package org.decopad.spike;

import android.content.Context;
import android.os.ParcelFileDescriptor;
import android.util.Log;

import java.io.BufferedOutputStream;
import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import moe.shizuku.server.IRemoteProcess;
import moe.shizuku.server.IShizukuService;
import rikka.shizuku.Shizuku;

/**
 * Fallback for ROMs where Shizuku user services never start (MediaTek's
 * LoadedApk.makeApplicationInner NPE). Shizuku starts the helper executable
 * from our APK's native library directory as uid shell; the helper owns
 * /dev/uhid and reads frames on stdin.
 */
final class ShellPen implements PenTransport {

    private static final String TAG = "decopad-helper";
    private static final int MAGIC = 0x50434544;
    private static final int TYPE_CREATE = 1;
    private static final int TYPE_INPUT = 2;
    private static final int TYPE_DESTROY = 3;
    private static final int TYPE_ACTION = 4;

    private final IRemoteProcess remote;
    private final OutputStream output;
    private final CountDownLatch ready = new CountDownLatch(1);
    private volatile CountDownLatch created;
    private volatile int createResult = Integer.MIN_VALUE;
    private volatile boolean open;

    static ShellPen start(Context context) throws Exception {
        String executable = context.getApplicationInfo().nativeLibraryDir
                + "/libdecopad_uhid_helper.so";
        if (!new File(executable).exists()) {
            throw new IllegalStateException("helper missing: " + executable);
        }
        IShizukuService service = IShizukuService.Stub.asInterface(Shizuku.getBinder());
        if (service == null) throw new IllegalStateException("Shizuku service unavailable");

        IRemoteProcess remote = service.newProcess(
                new String[]{executable}, null, "/data/local/tmp");
        Log.i(TAG, "started " + executable);
        ShellPen pen = new ShellPen(remote);
        if (!pen.ready.await(3, TimeUnit.SECONDS)) {
            pen.destroy();
            throw new IllegalStateException("helper did not announce READY");
        }
        return pen;
    }

    private ShellPen(IRemoteProcess remote) throws IOException, android.os.RemoteException {
        this.remote = remote;
        this.output = new BufferedOutputStream(new ParcelFileDescriptor.AutoCloseOutputStream(
                remote.getOutputStream()), 8192);
        InputStream stdout = new ParcelFileDescriptor.AutoCloseInputStream(
                remote.getInputStream());
        InputStream stderr = new ParcelFileDescriptor.AutoCloseInputStream(
                remote.getErrorStream());
        daemon("helper-out", () -> readStdout(stdout));
        daemon("helper-err", () -> readStderr(stderr));
    }

    private static void daemon(String name, Runnable body) {
        Thread thread = new Thread(body, name);
        thread.setDaemon(true);
        thread.start();
    }

    private void readStdout(InputStream stream) {
        try (BufferedReader reader = new BufferedReader(
                new InputStreamReader(stream, StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null) {
                Log.i(TAG, line);
                if ("READY".equals(line)) {
                    ready.countDown();
                } else if (line.startsWith("CREATED")) {
                    try {
                        createResult = Integer.parseInt(line.substring(8).trim());
                    } catch (NumberFormatException ignored) {
                        createResult = -1;
                    }
                    CountDownLatch latch = created;
                    if (latch != null) latch.countDown();
                }
            }
        } catch (IOException e) {
            Log.w(TAG, "stdout closed: " + e.getMessage());
        }
    }

    private void readStderr(InputStream stream) {
        try (BufferedReader reader = new BufferedReader(
                new InputStreamReader(stream, StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null) Log.w(TAG, "stderr: " + line);
        } catch (IOException ignored) {
        }
    }

    @Override
    public synchronized boolean create(byte[] descriptor) {
        CountDownLatch latch = new CountDownLatch(1);
        created = latch;
        createResult = Integer.MIN_VALUE;
        try {
            writeFrame(TYPE_CREATE, descriptor);
        } catch (IOException e) {
            Log.e(TAG, "create write failed", e);
            return false;
        }
        try {
            if (!latch.await(3, TimeUnit.SECONDS)) {
                Log.w(TAG, "create timed out");
                return false;
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return false;
        }
        open = createResult >= 0;
        Log.i(TAG, "create result=" + createResult);
        return open;
    }

    @Override
    public void submit(byte[] report) {
        if (!open || report == null) return;
        try {
            writeFrame(TYPE_INPUT, report);
        } catch (IOException e) {
            Log.w(TAG, "input write failed: " + e.getMessage());
        }
    }

    @Override
    public boolean isOpen() {
        return open;
    }

    @Override
    public void sendAction(int action) {
        try {
            ByteBuffer payload = ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN);
            payload.putInt(action);
            writeFrame(TYPE_ACTION, payload.array());
        } catch (IOException e) {
            Log.w(TAG, "action write failed: " + e.getMessage());
        }
    }

    @Override
    public synchronized void destroy() {
        open = false;
        try {
            writeFrame(TYPE_DESTROY, null);
        } catch (IOException ignored) {
        }
        try {
            output.close();
        } catch (IOException ignored) {
        }
        try {
            remote.destroy();
        } catch (Throwable t) {
            Log.w(TAG, "destroy process failed", t);
        }
    }

    private synchronized void writeFrame(int type, byte[] payload) throws IOException {
        int size = payload == null ? 0 : payload.length;
        byte[] frame = new byte[12 + size];
        ByteBuffer buffer = ByteBuffer.wrap(frame).order(ByteOrder.LITTLE_ENDIAN);
        buffer.putInt(MAGIC);
        buffer.putInt(type);
        buffer.putInt(size);
        if (size > 0) buffer.put(payload);
        output.write(frame);
        output.flush();
    }
}
