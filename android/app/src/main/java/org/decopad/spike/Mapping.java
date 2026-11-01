package org.decopad.spike;

/**
 * Pure area-mapping math. InputReader fits an external stylus so the axis
 * along the display's long side fills that side; the inverse of that scale
 * turns a wanted screen point into the raw values to send.
 */
final class Mapping {

    static final int STRETCH = 0;
    static final int FIT = 1;

    static final int TABLET_AUTO = -1;
    static final int TABLET_LANDSCAPE = 0;
    static final int TABLET_PORTRAIT_CCW = 90;
    static final int TABLET_PORTRAIT_CW = 270;

    static final int RAW_X = PenProtocol.MAX_X;
    static final int RAW_Y = PenProtocol.MAX_Y;

    /**
     * @param landscape whether the display is currently landscape (its long
     *                  side is the width)
     * @param region    normalized left, top, right, bottom of the screen
     * @return {rawX, rawY} to write into the report
     */
    static int[] map(int x, int y, int width, int height, boolean landscape,
                     int mode, int tablet,
                     float regionLeft, float regionTop, float regionRight, float regionBottom) {

        int effectiveTablet = tablet;
        if (effectiveTablet == TABLET_AUTO) {
            effectiveTablet = landscape ? TABLET_LANDSCAPE : TABLET_PORTRAIT_CW;
        }

        int tx;
        int ty;
        int tw;
        int th;
        if (effectiveTablet == TABLET_PORTRAIT_CCW) {
            tx = y;
            ty = RAW_X - x;
            tw = RAW_Y;
            th = RAW_X;
        } else if (effectiveTablet == TABLET_PORTRAIT_CW) {
            tx = RAW_Y - y;
            ty = x;
            tw = RAW_Y;
            th = RAW_X;
        } else {
            tx = x;
            ty = y;
            tw = RAW_X;
            th = RAW_Y;
        }

        float rl = regionLeft * width;
        float rt = regionTop * height;
        float rr = regionRight * width;
        float rb = regionBottom * height;

        float px;
        float py;
        if (mode == FIT) {
            float scale = Math.min((rr - rl) / tw, (rb - rt) / th);
            float cx = (rl + rr) / 2f;
            float cy = (rt + rb) / 2f;
            px = cx + (tx - tw / 2f) * scale;
            py = cy + (ty - th / 2f) * scale;
        } else {
            px = rl + (tx / (float) tw) * (rr - rl);
            py = rt + (ty / (float) th) * (rb - rt);
        }

        float scale = landscape
                ? Math.max(width, height) / (float) RAW_X
                : Math.max(width, height) / (float) RAW_Y;

        int outX = Math.round(px / scale);
        int outY = Math.round(py / scale);
        return new int[]{
                Math.max(0, Math.min(RAW_X, outX)),
                Math.max(0, Math.min(RAW_Y, outY))
        };
    }

    /**
     * Largest centered region with the tablet's aspect ratio, normalized to a
     * screen of the given size.
     *
     * @return {left, top, right, bottom}
     */
    static float[] aspectRegion(float screenWidth, float screenHeight) {
        float tablet = RAW_X / (float) RAW_Y;
        float screen = screenWidth / screenHeight;
        float w = 1f;
        float h = 1f;
        if (screen < tablet) {
            h = screen / tablet;
        } else {
            w = tablet / screen;
        }
        float l = (1f - w) / 2f;
        float t = (1f - h) / 2f;
        return new float[]{l, t, l + w, t + h};
    }

    private Mapping() {
    }
}
