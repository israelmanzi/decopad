package org.decopad.spike;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Point;
import android.hardware.display.DisplayManager;
import android.os.SystemClock;
import android.util.Log;
import android.view.Display;
import android.view.Surface;

/**
 * Maps the tablet's active area onto a screen region, then converts that
 * screen point back into the virtual stylus's raw coordinate space.
 *
 * InputReader fits an external stylus by scaling it so the axis that lines up
 * with the display's long side covers that side (measured on the phone):
 *
 * <pre>
 *   portrait  (rotation 0/2): pointer = raw * (max(W,H) / 8999)
 *   landscape (rotation 1/3): pointer = raw * (max(W,H) / 15999)
 * </pre>
 *
 * The pointer is in the current display's logical pixels; the mapping is
 * direct in all four rotations. The inverse of that scale is what turns a
 * wanted screen point into report bytes, so the whole tablet can be mapped to
 * any screen region regardless of the tablet's aspect ratio.
 */
final class AreaMapper {

    static final String TAG = "decopad-bridge";

    static final int STRETCH = Mapping.STRETCH;
    static final int FIT = Mapping.FIT;

    /** Tablet axes: follow the phone, or force one physical tablet rotation. */
    static final int TABLET_AUTO = Mapping.TABLET_AUTO;
    static final int TABLET_LANDSCAPE = Mapping.TABLET_LANDSCAPE;
    /** Tablet turned 90 degrees counter-clockwise. */
    static final int TABLET_PORTRAIT_CCW = Mapping.TABLET_PORTRAIT_CCW;
    /** Tablet turned 90 degrees clockwise. */
    static final int TABLET_PORTRAIT_CW = Mapping.TABLET_PORTRAIT_CW;

    static final int RAW_X = Mapping.RAW_X;
    static final int RAW_Y = Mapping.RAW_Y;

    private static final String PREFS = "mapping";
    private static final String KEY_MODE = "mode";
    private static final String KEY_TABLET = "tablet";
    private static final String KEY_LEFT = "left";
    private static final String KEY_TOP = "top";
    private static final String KEY_RIGHT = "right";
    private static final String KEY_BOTTOM = "bottom";

    static volatile int mode = STRETCH;
    static volatile int tablet = TABLET_AUTO;
    static volatile float left = 0f;
    static volatile float top = 0f;
    static volatile float right = 1f;
    static volatile float bottom = 1f;

    private final DisplayManager displays;
    private int width;
    private int height;
    private int rotation = -1;
    private long checked;

    AreaMapper(Context context) {
        displays = context.getSystemService(DisplayManager.class);
        load(context);
    }

    static void load(Context context) {
        SharedPreferences p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        mode = p.getInt(KEY_MODE, STRETCH) == FIT ? FIT : STRETCH;
        int savedTablet = p.getInt(KEY_TABLET, TABLET_AUTO);
        tablet = (savedTablet == TABLET_LANDSCAPE || savedTablet == TABLET_PORTRAIT_CCW
                || savedTablet == TABLET_PORTRAIT_CW) ? savedTablet : TABLET_AUTO;

        float savedLeft = p.getFloat(KEY_LEFT, 0f);
        float savedTop = p.getFloat(KEY_TOP, 0f);
        float savedRight = p.getFloat(KEY_RIGHT, 1f);
        float savedBottom = p.getFloat(KEY_BOTTOM, 1f);
        if (savedLeft < 0f || savedTop < 0f || savedRight > 1f || savedBottom > 1f
                || savedRight - savedLeft < 0.02f || savedBottom - savedTop < 0.02f) {
            savedLeft = 0f;
            savedTop = 0f;
            savedRight = 1f;
            savedBottom = 1f;
        }
        left = savedLeft;
        top = savedTop;
        right = savedRight;
        bottom = savedBottom;
    }

    static void setRegion(Context context, float newLeft, float newTop,
                          float newRight, float newBottom) {
        left = Math.max(0f, Math.min(1f, newLeft));
        top = Math.max(0f, Math.min(1f, newTop));
        right = Math.max(0f, Math.min(1f, newRight));
        bottom = Math.max(0f, Math.min(1f, newBottom));
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putFloat(KEY_LEFT, left)
                .putFloat(KEY_TOP, top)
                .putFloat(KEY_RIGHT, right)
                .putFloat(KEY_BOTTOM, bottom)
                .apply();
    }

    static void setMode(Context context, int value) {
        mode = value;
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit().putInt(KEY_MODE, value).apply();
    }

    static void setTablet(Context context, int value) {
        tablet = value;
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit().putInt(KEY_TABLET, value).apply();
    }

    static int nextTablet(int current) {
        switch (current) {
            case TABLET_LANDSCAPE:
                return TABLET_PORTRAIT_CCW;
            case TABLET_PORTRAIT_CCW:
                return TABLET_PORTRAIT_CW;
            case TABLET_PORTRAIT_CW:
                return TABLET_AUTO;
            default:
                return TABLET_LANDSCAPE;
        }
    }

    static String tabletName(int value) {
        switch (value) {
            case TABLET_LANDSCAPE:
                return "landscape";
            case TABLET_PORTRAIT_CCW:
                return "portrait CCW";
            case TABLET_PORTRAIT_CW:
                return "portrait CW";
            default:
                return "auto";
        }
    }

    /** Call with a fresh report (report ID 7) of at least 10 bytes. */
    void apply(byte[] report, int length) {
        if (!PenProtocol.isPenReport(report, length)) return;
        int x = Math.min(PenProtocol.x(report), RAW_X);
        int y = Math.min(PenProtocol.y(report), RAW_Y);
        int[] mapped = map(x, y);
        PenProtocol.setX(report, mapped[0]);
        PenProtocol.setY(report, mapped[1]);
    }

    int[] map(int x, int y) {
        refresh();
        boolean landscape = rotation == Surface.ROTATION_90
                || rotation == Surface.ROTATION_270;
        return Mapping.map(x, y, width, height, landscape, mode, tablet,
                left, top, right, bottom);
    }

    private void refresh() {
        long now = SystemClock.elapsedRealtime();
        if (width > 0 && now - checked < 500) return;
        checked = now;

        Display display = displays != null
                ? displays.getDisplay(Display.DEFAULT_DISPLAY) : null;
        if (display == null) {
            width = 1080;
            height = 2400;
            rotation = Surface.ROTATION_0;
            return;
        }
        int newRotation = display.getRotation();
        Point size = new Point();
        display.getRealSize(size);
        int w = size.x;
        int h = size.y;
        boolean landscape = newRotation == Surface.ROTATION_90
                || newRotation == Surface.ROTATION_270;
        if (landscape != (w > h)) {
            int swap = w;
            w = h;
            h = swap;
        }
        if (w != width || h != height || newRotation != rotation) {
            width = w;
            height = h;
            rotation = newRotation;
            Log.i(TAG, "mapping display=" + w + "x" + h + " rotation=" + newRotation
                    + " mode=" + (mode == FIT ? "fit" : "stretch")
                    + " tablet=" + tabletName(tablet));
        }
    }
}
