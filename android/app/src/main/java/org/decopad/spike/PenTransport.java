package org.decopad.spike;

/** Where the virtual stylus on /dev/uhid lives. */
interface PenTransport {

    int ACTION_BACKSPACE = PenProtocol.ACTION_BACKSPACE;
    int ACTION_UNDO = PenProtocol.ACTION_UNDO;

    boolean create(byte[] descriptor);

    void submit(byte[] report);

    boolean isOpen();

    /** Inject one action (ACTION_BACKSPACE or ACTION_UNDO) from the shell. */
    void sendAction(int action);

    void destroy();
}
