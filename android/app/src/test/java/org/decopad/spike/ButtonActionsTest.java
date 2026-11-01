package org.decopad.spike;

import static org.junit.Assert.assertEquals;

import org.junit.Test;

public class ButtonActionsTest {

    @Test
    public void topButtonIsOneShotUndo() {
        ButtonActions actions = new ButtonActions();
        assertEquals(PenProtocol.ACTION_UNDO,
                actions.onReport(PenProtocol.BUTTON_TOP, 1000));
        assertEquals(0, actions.onReport(PenProtocol.BUTTON_TOP, 1100));
        assertEquals(0, actions.onReport(0, 1200));
        assertEquals(PenProtocol.ACTION_UNDO,
                actions.onReport(PenProtocol.BUTTON_TOP, 2000));
    }

    @Test
    public void bottomButtonIsBackspace() {
        ButtonActions actions = new ButtonActions();
        assertEquals(PenProtocol.ACTION_BACKSPACE,
                actions.onReport(PenProtocol.BUTTON_BOTTOM, 1000));
        assertEquals(0, actions.onReport(PenProtocol.BUTTON_BOTTOM, 1100));
    }

    @Test
    public void outOfRangeBlipDoesNotDoubleFire() {
        ButtonActions actions = new ButtonActions();
        assertEquals(PenProtocol.ACTION_UNDO,
                actions.onReport(PenProtocol.BUTTON_TOP, 1000));
        // The tablet drops the pen out of range for ~40 ms and returns with
        // the button held; that must not become a second action.
        assertEquals(0, actions.onReport(0, 1010));
        assertEquals(0, actions.onReport(PenProtocol.BUTTON_TOP, 1040));
        // A genuinely new press after the debounce does fire.
        assertEquals(0, actions.onReport(0, 1400));
        assertEquals(PenProtocol.ACTION_UNDO,
                actions.onReport(PenProtocol.BUTTON_TOP, 1410));
    }
}
