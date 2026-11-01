package org.decopad.spike;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public class PenProtocolTest {

    private static byte[] report(int status, int x, int y) {
        byte[] report = new byte[PenProtocol.REPORT_LENGTH];
        report[0] = (byte) PenProtocol.REPORT_ID;
        report[1] = (byte) status;
        PenProtocol.setX(report, x);
        PenProtocol.setY(report, y);
        return report;
    }

    @Test
    public void parsesReport7() {
        byte[] report = report(0xa2, 8700, 4095);
        assertTrue(PenProtocol.isPenReport(report, report.length));
        assertEquals(8700, PenProtocol.x(report));
        assertEquals(4095, PenProtocol.y(report));
        assertEquals(PenProtocol.BUTTON_TOP, PenProtocol.buttons(report));
        assertFalse(PenProtocol.isPenReport(report, 9));
    }

    @Test
    public void masksBothButtonsButKeepsTheStatusBits() {
        byte[] report = report(0xa6, 0, 0);
        PenProtocol.maskButtons(report);
        assertEquals(0xa0, PenProtocol.status(report));
        assertEquals(0, PenProtocol.buttons(report));
    }

    @Test
    public void roundTripsCoordinates() {
        byte[] report = report(0x20, 0, 0);
        PenProtocol.setX(report, 15999);
        PenProtocol.setY(report, 8999);
        assertEquals(15999, PenProtocol.x(report));
        assertEquals(8999, PenProtocol.y(report));
    }
}
