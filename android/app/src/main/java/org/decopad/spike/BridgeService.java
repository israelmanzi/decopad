package org.decopad.spike;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.ComponentName;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.hardware.usb.UsbConstants;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;
import android.hardware.usb.UsbInterface;
import android.hardware.usb.UsbManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;

import org.decopad.pen.IPenService;
import org.decopad.pen.PenUserService;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import rikka.shizuku.Shizuku;

/**
 * Foreground owner of the USB tablet. Claims the tablet's HID interfaces so
 * the kernel no longer feeds Android a (wrongly mapped) stylus, and forwards
 * the raw pen reports to a virtual stylus on /dev/uhid.
 *
 * The virtual stylus normally lives in a Shizuku user service. Some ROMs
 * (notably MediaTek) cannot start Shizuku user services at all, so after a
 * timeout the bridge falls back to a helper executable that Shizuku runs as
 * uid shell.
 */
public class BridgeService extends Service {

    static final String TAG = "decopad-bridge";
    static final String ACTION_START = "org.decopad.spike.action.START";
    static final String ACTION_STOP = "org.decopad.spike.action.STOP";
    static final String ACTION_TEST = "org.decopad.spike.action.TEST";
    static final String EXTRA_DEVICE = "org.decopad.spike.extra.DEVICE";

    static final int VENDOR = 0x28bd;
    static final int PRODUCT = 0x2904;

    private static final int USER_SERVICE_TIMEOUT_MS = 4000;
    private static final int NOTIFICATION_ID = 1;
    private static final String CHANNEL_ID = "bridge";

    static volatile String status = "idle";

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final List<UsbInterface> claimed = new ArrayList<>();

    private UsbManager usbManager;
    private AreaMapper areaMapper;
    private String deviceName;
    private boolean testPen;

    private UsbDeviceConnection connection;
    private Thread reader;
    private volatile boolean reading;
    private long reports;
    private byte[] activeDescriptor;
    private final ButtonActions buttonActions = new ButtonActions();

    private PenTransport transport;
    private String transportKind = "none";

    private Shizuku.UserServiceArgs userServiceArgs;
    private boolean binding;
    private boolean started;

    private final Runnable userServiceTimeout = () -> {
        if (transport != null) return;
        Log.w(TAG, "Shizuku user service did not start; using the shell helper");
        startShellPen();
    };

    private final ServiceConnection penConnection = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder binder) {
            binding = false;
            handler.removeCallbacks(userServiceTimeout);
            if (transport != null) {
                Log.i(TAG, "user service connected after fallback started; ignoring");
                return;
            }
            transport = new UserServicePen(IPenService.Stub.asInterface(binder));
            transportKind = "user service";
            Log.i(TAG, "user service connected");
            if (testPen) runPenTest();
            else openTablet();
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            binding = false;
            Log.w(TAG, "user service disconnected");
            if (transport instanceof UserServicePen) {
                transport = null;
                closeUsb(true);
                status = "user service disconnected; restarting";
                startShellPen();
            }
        }
    };

    private final Shizuku.OnBinderReceivedListener binderReceivedListener = () -> {
        Log.i(TAG, "Shizuku binder received");
        if (started) bindPen();
    };

    private final Shizuku.OnBinderDeadListener binderDeadListener = () -> {
        Log.w(TAG, "Shizuku binder died");
        closeUsb(true);
        transport = null;
        status = "Shizuku stopped";
    };

    @Override
    public void onCreate() {
        super.onCreate();
        usbManager = getSystemService(UsbManager.class);
        areaMapper = new AreaMapper(this);
        createNotificationChannel();
        Shizuku.addBinderReceivedListenerSticky(binderReceivedListener);
        Shizuku.addBinderDeadListener(binderDeadListener);
        status = "service started";
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent != null ? intent.getAction() : ACTION_START;
        if (ACTION_STOP.equals(action)) {
            closeUsb(true);
            stopForeground(STOP_FOREGROUND_REMOVE);
            stopSelf();
            return START_NOT_STICKY;
        }

        testPen = ACTION_TEST.equals(action);
        // The self test runs from a visible activity and needs no USB access,
        // so it does not need a foreground service (whose connectedDevice
        // type requires a USB permission).
        if (!testPen) startForeground(NOTIFICATION_ID, buildNotification());
        if (!testPen) {
            deviceName = intent != null ? intent.getStringExtra(EXTRA_DEVICE) : null;
        }
        status = testPen ? "binding user service (test)" : "binding user service";
        started = true;
        bindPen();
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        closeUsb(true);
        handler.removeCallbacksAndMessages(null);
        Shizuku.removeBinderReceivedListener(binderReceivedListener);
        Shizuku.removeBinderDeadListener(binderDeadListener);
        if (userServiceArgs != null) {
            try {
                Shizuku.unbindUserService(userServiceArgs, penConnection, true);
            } catch (Throwable t) {
                Log.w(TAG, "unbind failed", t);
            }
        }
        status = "service stopped";
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    // ---- virtual pen ownership ------------------------------------------

    private void bindPen() {
        if (transport != null) {
            if (testPen) runPenTest();
            else openTablet();
            return;
        }
        if (!Shizuku.pingBinder()) {
            status = "waiting for Shizuku";
            Log.i(TAG, "Shizuku binder not available");
            return;
        }
        if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) {
            status = "Shizuku permission not granted";
            Log.i(TAG, "Shizuku permission not granted");
            return;
        }
        if (binding) return;

        userServiceArgs = new Shizuku.UserServiceArgs(
                new ComponentName(getPackageName(), PenUserService.class.getName()))
                .daemon(false)
                .processNameSuffix("pen")
                .debuggable((getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0)
                .version(1);
        try {
            Shizuku.bindUserService(userServiceArgs, penConnection);
            binding = true;
            handler.removeCallbacks(userServiceTimeout);
            handler.postDelayed(userServiceTimeout, USER_SERVICE_TIMEOUT_MS);
            Log.i(TAG, "binding user service");
        } catch (Throwable t) {
            Log.e(TAG, "bindUserService failed", t);
            startShellPen();
        }
    }

    private void startShellPen() {
        if (transport != null) return;
        status = "starting helper process";
        new Thread(() -> {
            try {
                ShellPen shellPen = ShellPen.start(this);
                synchronized (BridgeService.this) {
                    if (transport != null) {
                        shellPen.destroy();
                        return;
                    }
                    transport = shellPen;
                }
                transportKind = "helper process";
                Log.i(TAG, "helper process ready");
                if (testPen) runPenTest();
                else openTablet();
            } catch (Throwable t) {
                Log.e(TAG, "helper process failed", t);
                status = "helper failed: " + t.getMessage();
            }
        }, "shell-pen").start();
    }

    // ---- USB tablet ------------------------------------------------------

    private synchronized void openTablet() {
        if (connection != null || reading) return;
        if (transport == null) return;

        UsbDevice device = findTablet();
        if (device == null) {
            status = "waiting for tablet";
            Log.i(TAG, "tablet not attached");
            return;
        }
        deviceName = device.getDeviceName();
        if (!usbManager.hasPermission(device)) {
            status = "USB permission missing";
            Log.w(TAG, "no USB permission for " + deviceName);
            return;
        }

        UsbDeviceConnection opened = usbManager.openDevice(device);
        if (opened == null) {
            status = "openDevice failed";
            Log.e(TAG, "openDevice returned null");
            return;
        }

        PenInfo info = scanDescriptors(opened, device);
        if (info.descriptor == null) {
            // A previous owner may have left the device in a state where HID
            // GET_DESCRIPTOR stalls; one fresh connection usually recovers it.
            Log.w(TAG, "no pen report descriptor, reopening USB and retrying");
            opened.close();
            try {
                Thread.sleep(100);
            } catch (InterruptedException ignored) {
            }
            opened = usbManager.openDevice(device);
            if (opened == null) {
                status = "openDevice failed (retry)";
                return;
            }
            info = scanDescriptors(opened, device);
        }

        UsbInterface penInterface = info.intf;
        byte[] descriptor = info.descriptor;
        if (descriptor == null) {
            // Built-in descriptor is the tablet's own; only the interface has
            // to be guessed. Interface 1 is the pen on the Deco 640.
            descriptor = PenDescriptors.FALLBACK;
            penInterface = null;
            UsbInterface second = device.getInterfaceCount() > 1
                    ? device.getInterface(1) : null;
            if (second != null
                    && second.getInterfaceClass() == UsbConstants.USB_CLASS_HID
                    && hasInEndpoint(second)) {
                penInterface = second;
            }
            if (penInterface == null) {
                for (int i = 0; i < device.getInterfaceCount(); i++) {
                    UsbInterface intf = device.getInterface(i);
                    if (intf.getInterfaceClass() == UsbConstants.USB_CLASS_HID
                            && hasInEndpoint(intf)) {
                        penInterface = intf;
                        break;
                    }
                }
            }
            Log.w(TAG, "pen descriptor unreadable, using built-in descriptor"
                    + (penInterface != null ? " on interface " + penInterface.getId() : ""));
        }
        if (penInterface == null) {
            status = "no HID interface found";
            opened.close();
            return;
        }

        boolean created;
        try {
            activeDescriptor = descriptor;
            created = transport.create(descriptor);
        } catch (Throwable t) {
            Log.e(TAG, "create virtual pen failed", t);
            created = false;
        }
        if (!created) {
            status = "uhid create failed (" + transportKind + ")";
            opened.close();
            return;
        }

        connection = opened;
        claimed.clear();
        for (int i = 0; i < device.getInterfaceCount(); i++) {
            UsbInterface intf = device.getInterface(i);
            boolean ok = opened.claimInterface(intf, true);
            Log.i(TAG, "claim interface " + i + " (force) -> " + ok);
            if (ok) claimed.add(intf);
        }
        if (!claimed.contains(penInterface)) {
            status = "could not claim pen interface";
            closeUsb(false);
            return;
        }

        startReader(penInterface);
        status = "forwarding via " + transportKind;
        Log.i(TAG, "forwarding pen reports from interface " + penInterface.getId());
    }

    private UsbDevice findTablet() {
        for (UsbDevice device : usbManager.getDeviceList().values()) {
            if (device.getVendorId() != VENDOR || device.getProductId() != PRODUCT) continue;
            if (deviceName == null || deviceName.equals(device.getDeviceName())) return device;
        }
        return null;
    }

    private static final class PenInfo {
        UsbInterface intf;
        byte[] descriptor;
    }

    private static PenInfo scanDescriptors(UsbDeviceConnection connection, UsbDevice device) {
        PenInfo info = new PenInfo();
        for (int i = 0; i < device.getInterfaceCount(); i++) {
            UsbInterface intf = device.getInterface(i);
            if (intf.getInterfaceClass() != UsbConstants.USB_CLASS_HID) continue;
            byte[] reportDescriptor = readReportDescriptor(connection, intf, i);
            if (reportDescriptor == null) {
                Log.i(TAG, "interface " + i + ": no report descriptor");
                continue;
            }
            boolean isPen = hasUsagePage(reportDescriptor, 0x0d)
                    && hasReportId(reportDescriptor, 7);
            Log.i(TAG, "interface " + i + ": report descriptor "
                    + reportDescriptor.length + "B"
                    + (isPen ? " (pen) " + hex(reportDescriptor, reportDescriptor.length)
                             : " " + hex(reportDescriptor, 16) + " ..."));
            if (info.descriptor == null && isPen) {
                info.intf = intf;
                info.descriptor = reportDescriptor;
            }
        }
        return info;
    }

    private static byte[] readReportDescriptor(UsbDeviceConnection connection,
                                               UsbInterface intf, int index) {
        int expected = hidReportDescriptorLength(connection, intf.getId());
        int length = expected > 0 ? Math.min(expected, 4096) : 4096;
        byte[] buffer = new byte[Math.max(length, 4096)];
        int[] indexes = {intf.getId(), index};
        for (int wIndex : indexes) {
            Arrays.fill(buffer, (byte) 0);
            int n = connection.controlTransfer(0x81, 0x06, 0x22 << 8, wIndex,
                    buffer, length, 1000);
            Log.i(TAG, "interface " + index + ": GET_DESCRIPTOR wIndex=" + wIndex
                    + " expected=" + expected + " -> " + n);
            if (n > 0) return Arrays.copyOf(buffer, n);
        }
        return null;
    }

    /** wDescriptorLength of the HID descriptor for an interface, or -1. */
    private static int hidReportDescriptorLength(UsbDeviceConnection connection,
                                                 int interfaceNumber) {
        byte[] raw = connection.getRawDescriptors();
        if (raw == null) return -1;
        int i = 0;
        while (i + 2 <= raw.length) {
            int length = raw[i] & 0xff;
            if (length < 2) break;
            int type = raw[i + 1] & 0xff;
            if (type == 0x04 && i + 9 <= raw.length) {
                int number = raw[i + 2] & 0xff;
                if (number == interfaceNumber) {
                    int j = i + length;
                    while (j + 2 <= raw.length) {
                        int descriptorLength = raw[j] & 0xff;
                        int descriptorType = raw[j + 1] & 0xff;
                        if (descriptorLength < 2) break;
                        if (descriptorType == 0x21 && descriptorLength >= 9
                                && j + 9 <= raw.length) {
                            return (raw[j + 7] & 0xff) | ((raw[j + 8] & 0xff) << 8);
                        }
                        if (descriptorType == 0x04 || descriptorType == 0x05) break;
                        j += descriptorLength;
                    }
                }
            }
            i += length;
        }
        return -1;
    }

    private static boolean hasInEndpoint(UsbInterface intf) {
        for (int i = 0; i < intf.getEndpointCount(); i++) {
            if (intf.getEndpoint(i).getDirection() == UsbConstants.USB_DIR_IN) return true;
        }
        return false;
    }

    private static boolean hasUsagePage(byte[] descriptor, int page) {
        for (int i = 0; i + 1 < descriptor.length; i++) {
            if ((descriptor[i] & 0xff) == 0x05 && (descriptor[i + 1] & 0xff) == page) return true;
        }
        return false;
    }

    private static boolean hasReportId(byte[] descriptor, int id) {
        for (int i = 0; i + 1 < descriptor.length; i++) {
            if ((descriptor[i] & 0xff) == 0x85 && (descriptor[i + 1] & 0xff) == id) return true;
        }
        return false;
    }

    // ---- report forwarding ----------------------------------------------

    private void startReader(UsbInterface intf) {
        UsbEndpoint endpoint = null;
        for (int i = 0; i < intf.getEndpointCount(); i++) {
            UsbEndpoint candidate = intf.getEndpoint(i);
            if (candidate.getDirection() == UsbConstants.USB_DIR_IN
                    && candidate.getType() == UsbConstants.USB_ENDPOINT_XFER_INT) {
                endpoint = candidate;
                break;
            }
        }
        if (endpoint == null) {
            for (int i = 0; i < intf.getEndpointCount(); i++) {
                UsbEndpoint candidate = intf.getEndpoint(i);
                if (candidate.getDirection() == UsbConstants.USB_DIR_IN) {
                    endpoint = candidate;
                    break;
                }
            }
        }
        if (endpoint == null) {
            status = "pen interface has no IN endpoint";
            return;
        }

        final UsbEndpoint in = endpoint;
        reports = 0;
        buttonActions.reset();
        reading = true;
        reader = new Thread(() -> readLoop(in), "pen-reader");
        reader.start();
    }

    private void forward(byte[] report, int length) {
        if (PenProtocol.isPenReport(report, length)) {
            int action = buttonActions.onReport(PenProtocol.buttons(report),
                    SystemClock.elapsedRealtime());
            // The barrel buttons drive actions; hide them from apps so the
            // eraser bit cannot flip a drawing app into erase mode.
            PenProtocol.maskButtons(report);
            if (action != 0) {
                Log.i(TAG, "pen button -> "
                        + (action == PenProtocol.ACTION_BACKSPACE ? "backspace" : "undo"));
                PenTransport current = transport;
                if (current != null) current.sendAction(action);
            }
        }

        if (areaMapper != null) areaMapper.apply(report, length);
        PenTransport current = transport;
        if (current != null) current.submit(report);
    }

    private void readLoop(UsbEndpoint endpoint) {
        byte[] buffer = new byte[64];
        int failures = 0;
        long lastCheck = SystemClock.elapsedRealtime();

        while (reading) {
            UsbDeviceConnection c = connection;
            if (c == null) break;
            int n = c.bulkTransfer(endpoint, buffer, buffer.length, 250);
            if (n > 0) {
                byte[] report = Arrays.copyOf(buffer, n);
                forward(report, n);
                reports++;
                if (reports == 1 || reports % 200 == 0) {
                    Log.i(TAG, "reports=" + reports + " last=" + hex(report, n));
                }
                failures = 0;
                continue;
            }

            failures++;
            if (reports == 0 && failures == 20) {
                Log.i(TAG, "no USB pen reports yet (pen out of range?)");
            }
            long now = SystemClock.elapsedRealtime();
            if (failures >= 6 && now - lastCheck >= 2000) {
                lastCheck = now;
                if (findTablet() == null) {
                    Log.i(TAG, "tablet detached");
                    status = "tablet detached";
                    break;
                }
            }
        }

        closeUsb(false);
        if (status.startsWith("forwarding")) status = "reader stopped";
    }

    private void closeUsb(boolean joinReader) {
        reading = false;
        buttonActions.reset();

        Thread currentReader;
        PenTransport currentTransport;
        UsbDeviceConnection currentConnection;
        List<UsbInterface> interfaces;
        synchronized (this) {
            currentReader = reader;
            reader = null;
            currentTransport = transport;
            transport = null;
            transportKind = "none";
            currentConnection = connection;
            connection = null;
            interfaces = new ArrayList<>(claimed);
            claimed.clear();
        }

        if (joinReader && currentReader != null && currentReader != Thread.currentThread()) {
            try {
                currentReader.join(1000);
            } catch (InterruptedException ignored) {
            }
        }
        if (currentTransport != null) currentTransport.destroy();
        if (currentConnection != null) {
            for (UsbInterface intf : interfaces) {
                try {
                    currentConnection.releaseInterface(intf);
                } catch (Throwable ignored) {
                }
            }
            currentConnection.close();
        }
    }

    // ---- built-in self test ---------------------------------------------

    private void runPenTest() {
        PenTransport current = transport;
        if (current == null) return;
        testPen = false;
        try {
            if (!current.create(PenDescriptors.FALLBACK)) {
                status = "test: uhid create failed (" + transportKind + ")";
                return;
            }
            status = "test: virtual pen created via " + transportKind;
            new Thread(() -> {
                try {
                    Thread.sleep(500);
                    for (int i = 0; i < 150; i++) {
                        int x = 4000 + i * 60;
                        byte[] report = PenDescriptors.hoverReport(x, 4500);
                        forward(report, report.length);
                        Thread.sleep(50);
                    }
                    byte[] out = PenDescriptors.outOfRangeReport();
                    forward(out, out.length);
                    // Put the real tablet descriptor back if the bridge owns one.
                    byte[] real = activeDescriptor;
                    if (real != null) current.create(real);
                } catch (Throwable t) {
                    Log.w(TAG, "test reports failed", t);
                }
            }, "pen-test").start();
        } catch (Throwable t) {
            Log.e(TAG, "test failed", t);
            status = "test failed: " + t;
        }
    }

    // ---- notification ----------------------------------------------------

    private void createNotificationChannel() {
        NotificationChannel channel = new NotificationChannel(CHANNEL_ID, "decopad bridge",
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription("Forwards the tablet pen to the virtual stylus");
        getSystemService(NotificationManager.class).createNotificationChannel(channel);
    }

    private Notification buildNotification() {
        return new Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle("decopad")
                .setContentText("Pen bridge running")
                .setOngoing(true)
                .build();
    }

    static String hex(byte[] data, int length) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < length && i < data.length; i++) {
            sb.append(String.format("%02x ", data[i] & 0xff));
        }
        return sb.toString().trim();
    }
}
