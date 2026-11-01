package org.decopad.spike;

import android.util.Log;

import org.decopad.pen.IPenService;

/** The normal path: uhid lives in the Shizuku user service. */
final class UserServicePen implements PenTransport {

    private static final String TAG = "decopad-bridge";

    private final IPenService service;

    UserServicePen(IPenService service) {
        this.service = service;
    }

    @Override
    public boolean create(byte[] descriptor) {
        try {
            return service.create(descriptor);
        } catch (Throwable t) {
            Log.e(TAG, "user service create failed", t);
            return false;
        }
    }

    @Override
    public void submit(byte[] report) {
        try {
            service.submit(report);
        } catch (Throwable t) {
            Log.e(TAG, "user service submit failed", t);
        }
    }

    @Override
    public boolean isOpen() {
        try {
            return service.isOpen();
        } catch (Throwable t) {
            return false;
        }
    }

    @Override
    public void sendAction(int action) {
        try {
            service.sendAction(action);
        } catch (Throwable t) {
            Log.w(TAG, "user service sendAction failed", t);
        }
    }

    @Override
    public void destroy() {
        try {
            service.destroy();
        } catch (Throwable t) {
            Log.w(TAG, "user service destroy failed", t);
        }
    }
}
