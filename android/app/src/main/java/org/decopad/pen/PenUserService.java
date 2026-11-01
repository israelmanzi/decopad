package org.decopad.pen;

import android.content.Context;
import android.util.Log;

/**
 * Runs inside Shizuku's user service process (uid shell, group uhid) and owns
 * the virtual stylus on /dev/uhid. The tablet's raw report descriptors and
 * input reports are handed over from the app over binder, so this service
 * never touches USB.
 */
public class PenUserService extends IPenService.Stub {

    private static final String TAG = "decopad-uhid";
    private static final int VENDOR = 0x28bd;
    private static final int PRODUCT = 0x2904;

    private static boolean libraryLoaded;
    private static Context appContext;
    private long submitted;

    /** Shizuku instantiates this constructor when it is available. */
    public PenUserService(Context context) {
        appContext = context;
        loadLibrary();
        Log.i(TAG, "service up pid=" + android.os.Process.myPid()
                + " uid=" + android.os.Process.myUid());
    }

    public PenUserService() {
        loadLibrary();
        Log.i(TAG, "service up (no context) pid=" + android.os.Process.myPid()
                + " uid=" + android.os.Process.myUid());
    }

    private static synchronized void loadLibrary() {
        if (libraryLoaded) return;
        try {
            System.loadLibrary("decopad_uhid");
        } catch (UnsatisfiedLinkError e) {
            String dir = appContext != null
                    ? appContext.getApplicationInfo().nativeLibraryDir
                    : "/data/local/tmp";
            Log.w(TAG, "loadLibrary failed, trying " + dir, e);
            System.load(dir + "/libdecopad_uhid.so");
        }
        libraryLoaded = true;
    }

    @Override
    public boolean create(byte[] descriptor) {
        if (descriptor == null || descriptor.length == 0) {
            Log.e(TAG, "create: empty descriptor");
            return false;
        }
        int result = nativeCreate(descriptor, "XPPen Deco 640 Pen (decopad)",
                VENDOR, PRODUCT, 0x0100);
        Log.i(TAG, "create: descriptor=" + descriptor.length + "B, "
                + (result >= 0 ? "fd=" + result : "error=" + result));
        return result >= 0;
    }

    @Override
    public void submit(byte[] report) {
        if (report == null) return;
        int result = nativeSubmit(report);
        if (result != 0 && submitted++ % 100 == 0) {
            Log.w(TAG, "submit failed: " + result);
        }
    }

    @Override
    public boolean isOpen() {
        return nativeIsOpen();
    }

    @Override
    public void sendAction(int action) {
        // This process runs as uid shell, so /system/bin/input may inject.
        // Backspace only: this Android build has no "input keycombination",
        // so Ctrl+Z (undo) is implemented by the shell helper's keyboard.
        if (action != ACTION_BACKSPACE) {
            Log.w(TAG, "action " + action + " is not supported in the user service");
            return;
        }
        new Thread(() -> {
            try {
                Runtime.getRuntime().exec(new String[]{
                        "/system/bin/input", "keyevent", "67"});
            } catch (Throwable t) {
                Log.w(TAG, "sendAction failed", t);
            }
        }, "send-action").start();
    }

    @Override
    public void destroy() {
        nativeDestroy();
        Log.i(TAG, "destroy");
    }

    private static native int nativeCreate(byte[] descriptor, String name,
                                           int vendor, int product, int version);

    private static native int nativeSubmit(byte[] report);

    private static native boolean nativeIsOpen();

    private static native void nativeDestroy();
}
