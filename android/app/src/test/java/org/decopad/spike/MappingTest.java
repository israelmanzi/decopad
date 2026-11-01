package org.decopad.spike;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;

import org.junit.Test;

public class MappingTest {

    private static final int W = 1080;
    private static final int H = 2400;

    private static int[] portrait(int x, int y, int mode, int tablet) {
        return Mapping.map(x, y, W, H, false, mode, tablet, 0f, 0f, 1f, 1f);
    }

    private static int[] landscape(int x, int y, int mode, int tablet) {
        return Mapping.map(x, y, H, W, true, mode, tablet, 0f, 0f, 1f, 1f);
    }

    @Test
    public void portraitStretchFillsTheScreen() {
        int[] bottomRight = portrait(PenProtocol.MAX_X, PenProtocol.MAX_Y,
                Mapping.STRETCH, Mapping.TABLET_LANDSCAPE);
        assertEquals(4050, bottomRight[0], 1);
        assertEquals(8999, bottomRight[1], 1);

        assertArrayEquals(new int[]{0, 0},
                portrait(0, 0, Mapping.STRETCH, Mapping.TABLET_LANDSCAPE));
    }

    @Test
    public void portraitClockwiseRotatesTheAxes() {
        // Tablet's right edge maps to the bottom of the screen.
        int[] rightBottom = portrait(PenProtocol.MAX_X, PenProtocol.MAX_Y,
                Mapping.STRETCH, Mapping.TABLET_PORTRAIT_CW);
        assertArrayEquals(new int[]{0, 8999}, rightBottom);

        int[] leftTop = portrait(0, 0, Mapping.STRETCH, Mapping.TABLET_PORTRAIT_CW);
        assertEquals(4050, leftTop[0], 1);
        assertEquals(0, leftTop[1], 1);
    }

    @Test
    public void portraitFitIsCenteredAndAspectPreserving() {
        int[] centre = portrait(PenProtocol.MAX_X / 2, PenProtocol.MAX_Y / 2,
                Mapping.FIT, Mapping.TABLET_LANDSCAPE);
        assertEquals(2025, centre[0], 2);
        assertEquals(4500, centre[1], 2);
    }

    @Test
    public void landscapeStretchFitsTheWidth() {
        int[] bottomRight = landscape(PenProtocol.MAX_X, PenProtocol.MAX_Y,
                Mapping.STRETCH, Mapping.TABLET_LANDSCAPE);
        assertEquals(15999, bottomRight[0]);
        assertEquals(7200, bottomRight[1], 1);
    }

    @Test
    public void regionLimitsTheMapping() {
        int[] centre = Mapping.map(PenProtocol.MAX_X, PenProtocol.MAX_Y,
                W, H, false, Mapping.STRETCH, Mapping.TABLET_LANDSCAPE,
                0f, 0f, 0.5f, 0.5f);
        assertEquals(2025, centre[0], 1);
        assertEquals(4500, centre[1], 1);
    }

    @Test
    public void aspectRegionMatchesTheTablet() {
        float[] portrait = Mapping.aspectRegion(1080, 2400);
        assertEquals(0f, portrait[0], 0.001f);
        assertEquals(0.3734f, portrait[1], 0.001f);
        assertEquals(1f, portrait[2], 0.001f);
        assertEquals(0.6266f, portrait[3], 0.001f);

        float[] landscape = Mapping.aspectRegion(2400, 1080);
        assertEquals(0.1f, landscape[0], 0.002f);
        assertEquals(0f, landscape[1], 0.001f);
        assertEquals(0.9f, landscape[2], 0.002f);
        assertEquals(1f, landscape[3], 0.001f);
    }

    @Test
    public void autoFollowsTheDisplayOrientation() {
        assertArrayEquals(
                portrait(1234, 5678, Mapping.STRETCH, Mapping.TABLET_PORTRAIT_CW),
                portrait(1234, 5678, Mapping.STRETCH, Mapping.TABLET_AUTO));
        assertArrayEquals(
                landscape(1234, 5678, Mapping.STRETCH, Mapping.TABLET_LANDSCAPE),
                landscape(1234, 5678, Mapping.STRETCH, Mapping.TABLET_AUTO));
    }
}
