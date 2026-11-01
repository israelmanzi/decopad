package org.decopad.spike;

/**
 * Fallback HID report descriptor and synthetic reports for the Deco 640's
 * pen report 7. Kept identical to the tablet's real descriptor (see below), so
 * a forwarded raw report means the same thing whichever one is in use.
 */
final class PenDescriptors {

    /** In Range is bit 5 of the status byte, as in the real descriptor. */
    static final int STATUS_IN_RANGE = PenProtocol.STATUS_IN_RANGE;
    static final int MAX_X = PenProtocol.MAX_X;
    static final int MAX_Y = PenProtocol.MAX_Y;
    static final int MAX_PRESSURE = 16383;

    /**
     * The Deco 640 pen interface's real HID report descriptor (109 bytes),
     * read from the tablet with GET_DESCRIPTOR. Report 7:
     * <pre>
     *   byte 0      report id (7)
     *   byte 1      bit0 tip, bit1 barrel, bit2 eraser, bit3 invert,
     *               bit5 in-range
     *   bytes 2..3  X, 0..15999
     *   bytes 4..5  Y, 0..8999
     *   bytes 6..7  pressure, 0..16383 (null state allowed)
     *   byte 8      tilt X, -127..127
     *   byte 9      tilt Y, -127..127
     * </pre>
     */
    private static final int[] FALLBACK_ITEMS = {
            0x05, 0x0d, 0x09, 0x02, 0xa1, 0x01, 0x85, 0x07,
            0x09, 0x20, 0xa1, 0x01, 0x09, 0x42, 0x09, 0x44,
            0x09, 0x45, 0x09, 0x3c, 0x15, 0x00, 0x25, 0x01,
            0x75, 0x01, 0x95, 0x04, 0x81, 0x02, 0x95, 0x01,
            0x81, 0x03, 0x09, 0x32, 0x95, 0x01, 0x81, 0x02,
            0x95, 0x02, 0x81, 0x03, 0x75, 0x10, 0x95, 0x01,
            0x35, 0x00, 0xa4, 0x05, 0x01, 0x09, 0x30, 0x65,
            0x13, 0x55, 0x0d, 0x46, 0x9b, 0x18, 0x26, 0x7f,
            0x3e, 0x81, 0x02, 0x09, 0x31, 0x46, 0xd7, 0x0d,
            0x26, 0x27, 0x23, 0x81, 0x02, 0xb4, 0x09, 0x30,
            0x45, 0x00, 0x26, 0xff, 0x3f, 0x81, 0x42, 0x09,
            0x3d, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08, 0x95,
            0x01, 0x81, 0x02, 0x09, 0x3e, 0x15, 0x81, 0x25,
            0x7f, 0x81, 0x02, 0xc0, 0xc0,
    };

    static final byte[] FALLBACK = new byte[FALLBACK_ITEMS.length];

    static {
        for (int i = 0; i < FALLBACK_ITEMS.length; i++) {
            FALLBACK[i] = (byte) FALLBACK_ITEMS[i];
        }
    }

    static byte[] hoverReport(int x, int y) {
        byte[] report = new byte[PenProtocol.REPORT_LENGTH];
        report[0] = (byte) PenProtocol.REPORT_ID;
        report[1] = (byte) STATUS_IN_RANGE;
        PenProtocol.setX(report, x);
        PenProtocol.setY(report, y);
        return report;
    }

    static byte[] outOfRangeReport() {
        byte[] report = new byte[PenProtocol.REPORT_LENGTH];
        report[0] = (byte) PenProtocol.REPORT_ID;
        return report;
    }

    private PenDescriptors() {
    }
}
