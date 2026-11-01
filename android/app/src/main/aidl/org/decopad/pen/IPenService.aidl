package org.decopad.pen;

interface IPenService {

    const int ACTION_BACKSPACE = 1;
    const int ACTION_UNDO = 2;

    /**
     * Reserved destroy method defined by the Shizuku server (Shizuku calls
     * this when the user service is removed). Closes the virtual pen.
     */
    void destroy() = 16777114;

    /**
     * Creates the virtual stylus from an HID report descriptor.
     *
     * @return true when /dev/uhid accepted the device.
     */
    boolean create(in byte[] descriptor) = 1;

    /**
     * Forwards one raw HID input report (report ID included) from the tablet.
     */
    oneway void submit(in byte[] report) = 2;

    boolean isOpen() = 3;

    /**
     * Injects one action (ACTION_BACKSPACE or ACTION_UNDO) from the shell
     * context.
     */
    oneway void sendAction(int action) = 4;
}
