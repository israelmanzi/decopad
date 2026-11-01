package org.decopad.spike;

/** Layout of the Deco 640's pen report 7 (10 bytes, report ID included). */
final class PenProtocol {

    static final int REPORT_ID = 0x07;
    static final int REPORT_LENGTH = 10;

    static final int STATUS_IN_RANGE = 0x20;
    /** Status bit 1: the top/smaller button (undo). */
    static final int BUTTON_TOP = 0x02;
    /** Status bit 2: the bottom/larger button (backspace). */
    static final int BUTTON_BOTTOM = 0x04;
    static final int BUTTON_MASK = BUTTON_TOP | BUTTON_BOTTOM;

    static final int MAX_X = 15999;
    static final int MAX_Y = 8999;

    static final int ACTION_BACKSPACE = 1;
    static final int ACTION_UNDO = 2;

    static boolean isPenReport(byte[] report, int length) {
        return length >= REPORT_LENGTH && (report[0] & 0xff) == REPORT_ID;
    }

    static int status(byte[] report) {
        return report[1] & 0xff;
    }

    static int buttons(byte[] report) {
        return status(report) & BUTTON_MASK;
    }

    static void maskButtons(byte[] report) {
        report[1] = (byte) (status(report) & ~BUTTON_MASK);
    }

    static int x(byte[] report) {
        return (report[2] & 0xff) | ((report[3] & 0xff) << 8);
    }

    static int y(byte[] report) {
        return (report[4] & 0xff) | ((report[5] & 0xff) << 8);
    }

    static void setX(byte[] report, int value) {
        report[2] = (byte) value;
        report[3] = (byte) (value >> 8);
    }

    static void setY(byte[] report, int value) {
        report[4] = (byte) value;
        report[5] = (byte) (value >> 8);
    }

    private PenProtocol() {
    }
}
