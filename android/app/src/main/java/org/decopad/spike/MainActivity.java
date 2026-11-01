package org.decopad.spike;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.Gravity;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import rikka.shizuku.Shizuku;

/**
 * Status and permission UI. The actual bridge runs in {@link BridgeService};
 * validation happens in logcat and in the target drawing app.
 */
public class MainActivity extends Activity {

    static final String TAG = "decopad-spike";
    static final int REQUEST_SHIZUKU = 1;
    static final String ACTION_USB_PERMISSION = "org.decopad.spike.action.USB_PERMISSION";
    static final int VENDOR = 0x28bd;
    static final int PRODUCT = 0x2904;

    private final Handler handler = new Handler(Looper.getMainLooper());
    private UsbManager usbManager;
    private UsbDevice tablet;
    private TextView statusView;
    private Button modeButton;
    private Button tabletButton;
    private boolean pendingTest;

    private String lastStatus;

    private final Runnable poll = new Runnable() {
        @Override
        public void run() {
            if (statusView != null) {
                String status = describe();
                if (!status.equals(lastStatus)) {
                    lastStatus = status;
                    statusView.setText(status);
                }
            }
            handler.postDelayed(this, 500);
        }
    };

    private final BroadcastReceiver usbReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            String action = intent.getAction();
            if (ACTION_USB_PERMISSION.equals(action)) {
                UsbDevice device = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE,
                        UsbDevice.class);
                boolean granted = intent.getBooleanExtra(
                        UsbManager.EXTRA_PERMISSION_GRANTED, false);
                Log.i(TAG, "USB permission granted=" + granted);
                if (granted && device != null) {
                    tablet = device;
                    startBridge();
                }
            } else if (UsbManager.ACTION_USB_DEVICE_ATTACHED.equals(action)) {
                UsbDevice device = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE,
                        UsbDevice.class);
                if (device != null && device.getVendorId() == VENDOR
                        && device.getProductId() == PRODUCT) {
                    Log.i(TAG, "tablet attached " + device.getDeviceName());
                    tablet = device;
                    startBridge();
                }
            } else if (UsbManager.ACTION_USB_DEVICE_DETACHED.equals(action)) {
                UsbDevice device = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE,
                        UsbDevice.class);
                if (device != null && device.getDeviceName().equals(
                        tablet != null ? tablet.getDeviceName() : null)) {
                    Log.i(TAG, "tablet detached");
                    tablet = null;
                    stopService(new Intent(MainActivity.this, BridgeService.class));
                }
            }
        }
    };

    private final Shizuku.OnRequestPermissionResultListener permissionListener =
            (requestCode, grantResult) -> {
                if (requestCode != REQUEST_SHIZUKU) return;
                Log.i(TAG, "Shizuku permission result=" + grantResult);
                if (grantResult == PackageManager.PERMISSION_GRANTED) {
                    if (pendingTest) {
                        pendingTest = false;
                        startSelfTest();
                    } else {
                        startBridge();
                    }
                }
            };

    private final Shizuku.OnBinderReceivedListener binderReceivedListener =
            () -> Log.i(TAG, "Shizuku binder received");

    private final Shizuku.OnBinderDeadListener binderDeadListener =
            () -> Log.i(TAG, "Shizuku binder died");

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        usbManager = getSystemService(UsbManager.class);
        AreaMapper.load(this);

        IntentFilter filter = new IntentFilter(ACTION_USB_PERMISSION);
        filter.addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED);
        filter.addAction(UsbManager.ACTION_USB_DEVICE_DETACHED);
        registerReceiver(usbReceiver, filter, Context.RECEIVER_NOT_EXPORTED);

        Shizuku.addRequestPermissionResultListener(permissionListener);
        Shizuku.addBinderReceivedListenerSticky(binderReceivedListener);
        Shizuku.addBinderDeadListener(binderDeadListener);

        buildUi();
        tablet = findTablet();
        handler.post(poll);
        if (tablet != null) startBridge();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        Log.i(TAG, "onNewIntent " + intent);
        tablet = findTablet();
        startBridge();
    }

    @Override
    protected void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        unregisterReceiver(usbReceiver);
        Shizuku.removeRequestPermissionResultListener(permissionListener);
        Shizuku.removeBinderReceivedListener(binderReceivedListener);
        Shizuku.removeBinderDeadListener(binderDeadListener);
        super.onDestroy();
    }

    private void buildUi() {
        int pad = (int) (16 * getResources().getDisplayMetrics().density);

        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setPadding(pad, pad, pad, pad);

        statusView = new TextView(this);
        statusView.setTextSize(15);
        layout.addView(statusView);

        layout.addView(button("Grant Shizuku + start", v -> startBridge()));
        layout.addView(button("Stop bridge", v -> stopService(
                new Intent(this, BridgeService.class))));
        layout.addView(button("Test virtual pen (hover only)", v -> requestSelfTest()));

        modeButton = button("", v -> {
            AreaMapper.setMode(this, AreaMapper.mode == AreaMapper.FIT
                    ? AreaMapper.STRETCH : AreaMapper.FIT);
            updateMappingLabels();
        });
        layout.addView(modeButton);

        tabletButton = button("", v -> {
            AreaMapper.setTablet(this, AreaMapper.nextTablet(AreaMapper.tablet));
            updateMappingLabels();
        });
        layout.addView(tabletButton);

        layout.addView(button("Choose screen region...", v -> startActivity(
                new Intent(this, RegionPickerActivity.class))));
        updateMappingLabels();

        TextView hint = new TextView(this);
        hint.setText("Watch: adb logcat -s decopad-bridge decopad-uhid decopad-spike\n"
                + "or: adb shell getevent -lt");
        hint.setGravity(Gravity.START);
        hint.setTextSize(12);
        layout.addView(hint);

        // targetSdk 35 draws edge-to-edge: keep everything clear of the status
        // bar and the navigation/task bar.
        ScrollView scroll = new ScrollView(this);
        scroll.addView(layout);
        scroll.setOnApplyWindowInsetsListener((view, insets) -> {
            android.graphics.Insets bars = insets.getInsets(
                    android.view.WindowInsets.Type.systemBars());
            view.setPadding(0, bars.top, 0, bars.bottom);
            return insets;
        });

        setContentView(scroll);
    }

    private Button button(String label, android.view.View.OnClickListener listener) {
        Button button = new Button(this);
        button.setText(label);
        button.setOnClickListener(listener);
        return button;
    }

    private void updateMappingLabels() {
        if (modeButton != null) {
            modeButton.setText(AreaMapper.mode == AreaMapper.FIT
                    ? "Mapping: fit (tap for stretch)"
                    : "Mapping: stretch (tap for fit)");
        }
        if (tabletButton != null) {
            tabletButton.setText("Tablet axes: " + AreaMapper.tabletName(AreaMapper.tablet)
                    + " (tap to cycle)");
        }
    }

    private String describe() {
        StringBuilder sb = new StringBuilder();
        sb.append("Bridge: ").append(BridgeService.status).append('\n');
        if (!Shizuku.pingBinder()) {
            sb.append("Shizuku: not running\n");
        } else {
            boolean granted = Shizuku.checkSelfPermission()
                    == PackageManager.PERMISSION_GRANTED;
            sb.append("Shizuku: running (v").append(Shizuku.getVersion())
                    .append("), permission ").append(granted ? "granted" : "not granted")
                    .append('\n');
        }
        UsbDevice device = findTablet();
        sb.append("Tablet: ").append(device == null ? "not attached" : device.getDeviceName());
        if (device != null) {
            sb.append(usbManager.hasPermission(device) ? " (USB permission ok)" : " (no USB permission)");
        }
        sb.append("\nMapping: ")
                .append(AreaMapper.mode == AreaMapper.FIT ? "fit" : "stretch")
                .append(", tablet ")
                .append(AreaMapper.tabletName(AreaMapper.tablet))
                .append(String.format(java.util.Locale.US,
                        ", region L%d%% T%d%% R%d%% B%d%%",
                        Math.round(AreaMapper.left * 100), Math.round(AreaMapper.top * 100),
                        Math.round(AreaMapper.right * 100), Math.round(AreaMapper.bottom * 100)));
        return sb.toString();
    }

    private void startBridge() {
        tablet = findTablet();
        if (tablet == null) {
            Log.i(TAG, "start: no tablet");
            return;
        }
        if (!Shizuku.pingBinder()) {
            Log.w(TAG, "start: Shizuku not running");
            return;
        }
        if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) {
            pendingTest = false;
            Shizuku.requestPermission(REQUEST_SHIZUKU);
            return;
        }
        if (!usbManager.hasPermission(tablet)) {
            requestUsbPermission(tablet);
            return;
        }
        Intent intent = new Intent(this, BridgeService.class)
                .setAction(BridgeService.ACTION_START)
                .putExtra(BridgeService.EXTRA_DEVICE, tablet.getDeviceName());
        startForegroundService(intent);
        Log.i(TAG, "bridge service starting");
    }

    private void requestSelfTest() {
        if (!Shizuku.pingBinder()) {
            Log.w(TAG, "test: Shizuku not running");
            return;
        }
        if (Shizuku.checkSelfPermission() != PackageManager.PERMISSION_GRANTED) {
            pendingTest = true;
            Shizuku.requestPermission(REQUEST_SHIZUKU);
            return;
        }
        startSelfTest();
    }

    private void startSelfTest() {
        startService(new Intent(this, BridgeService.class)
                .setAction(BridgeService.ACTION_TEST));
        Log.i(TAG, "self test starting");
    }

    private void requestUsbPermission(UsbDevice device) {
        PendingIntent intent = PendingIntent.getBroadcast(this, 0,
                new Intent(ACTION_USB_PERMISSION).setPackage(getPackageName()),
                PendingIntent.FLAG_MUTABLE);
        usbManager.requestPermission(device, intent);
        Log.i(TAG, "requested USB permission");
    }

    private UsbDevice findTablet() {
        for (UsbDevice device : usbManager.getDeviceList().values()) {
            if (device.getVendorId() == VENDOR && device.getProductId() == PRODUCT) {
                return device;
            }
        }
        return null;
    }
}
