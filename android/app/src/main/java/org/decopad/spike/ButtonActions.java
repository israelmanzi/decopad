package org.decopad.spike;

/**
 * Turns the pen's barrel-button bits into one-shot actions. The upper button
 * makes the tablet blip out of range, so a short debounce prevents a double
 * action from the same physical press.
 */
final class ButtonActions {

    static final long DEBOUNCE_MS = 300;

    private int lastButtons;
    private long lastActionAt;

    /** @return 0, PenProtocol.ACTION_BACKSPACE or PenProtocol.ACTION_UNDO. */
    int onReport(int buttons, long now) {
        int pressed = buttons & ~lastButtons;
        lastButtons = buttons;
        if (pressed == 0 || now - lastActionAt <= DEBOUNCE_MS) return 0;

        if ((pressed & PenProtocol.BUTTON_TOP) != 0) {
            lastActionAt = now;
            return PenProtocol.ACTION_UNDO;
        }
        if ((pressed & PenProtocol.BUTTON_BOTTOM) != 0) {
            lastActionAt = now;
            return PenProtocol.ACTION_BACKSPACE;
        }
        return 0;
    }

    void reset() {
        lastButtons = 0;
        lastActionAt = 0;
    }
}
