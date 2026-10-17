# Domain Glossary

## Tablet
The XP-Pen Deco 640 drawing tablet (USB 28bd:2904): a screenless surface that reports the Pen's position, plus a row of Express Keys.

**Distinct from:** Trackpad (the laptop's built-in relative pointing device)

## Pen
The stylus used on the Tablet. It reports absolute position, pressure, tilt, hover (in range) and two Pen Buttons.

## Pen Button
One of the two buttons on the Pen's barrel. A click runs its action (Undo on the lower one, Redo on the upper one by default); holding the upper one while the Pen touches the Tablet erases instead.

**Distinct from:** Express Key (which lives on the Tablet body)

## Express Key
A physical shortcut button on the Tablet body.

**Distinct from:** Pen Button

## Active Area
The region of the Tablet surface where the Pen is tracked (about 6.3 × 3.5 inches).

## Factory Mode
The Tablet's power-on behaviour, where Express Keys emit fixed keyboard combinations chosen by the firmware.

**Distinct from:** Raw Mode

## Raw Mode
A Tablet state, entered by a vendor command, in which Express Keys report as plain button presses so software can give them any meaning.

**Distinct from:** Factory Mode

## Pointer
The system mouse arrow, shared by the mouse, Trackpad and Pen. While writing in the popup, the Pen does not move it.

**Distinct from:** Caret, Pen Tip

## Caret
The text insertion point inside a focused text field or editor.

**Distinct from:** Pointer

## Pen Tip
The Pen's current position as drawn inside the popup.

**Distinct from:** Pointer

## Anchor
The screen position of the Pointer at the moment of a Commit; Ink is reproduced relative to it. After a Replay the Pointer is left just past the delivered Ink, so the next Commit continues the same line unless the user moves the Pointer.

## Pad Mode
The mode in which the Popup is open: the Pen writes only into it and never moves the Pointer. Opening the Popup enters Pad Mode.

**Also known as:** writing pad open
**Distinct from:** Desktop Mode

## Desktop Mode
The mode in which the Popup is closed and the Pen behaves as an ordinary tablet pen, moving the Pointer and drawing directly in whatever app is under it. Closing the Popup enters Desktop Mode.

**Distinct from:** Pad Mode

## Ink
The set of strokes written in the popup since the last Commit or Clear.

## Commit
The act of reproducing the Ink at the Anchor in the target app and emptying the Popup, which stays open for more writing. Triggered by an Express Key or by a pause with the Pen out of range.

**Distinct from:** Clear (which empties the Popup without reproducing anything)

## Clear
Emptying the popup's Ink without reproducing it anywhere.

**Distinct from:** Commit, Undo (which removes one stroke)

## Replay
The default form of Commit: the Ink is redrawn stroke by stroke at the Anchor by a virtual pen, so the target app's own freehand tool records it.

**Distinct from:** Picture Paste

## Picture Paste
An alternative to Replay: the Ink is rendered as a transparent image and pasted into the target app, landing wherever that app places pastes (usually the Caret).

**Distinct from:** Replay

## Popup
The small always-on-top writing window, with no title bar or window buttons; it is open exactly while in Pad Mode. Express Key 1, the Popup Toggle and Esc open or close it; it stays open across Commits and keeps its Ink when closed. The mouse moves it by dragging and resizes it from its bottom-right corner; it reopens where it was left. It shows a subtle Cheatsheet.

**Also known as:** writing pad

## Popup Toggle
A keyboard shortcut (Super+W, where Super is the Windows key) that opens or closes the Popup, switching between Pad Mode and Desktop Mode, the same as Express Key 1.

## Cheatsheet
A faint, always-visible legend inside the Popup listing what each Express Key and shortcut does in the current mode.

## Ink Scale
How large Ink is when delivered, expressed as target-screen pixels per millimetre of Tablet surface (default 3 px/mm, so 6 mm handwriting arrives about 18 px tall, close to document text). Independent of the Popup's on-screen size; adjustable with Express Keys 5 and 6 while the Popup is open.

## Writing
The state in which the Popup takes Enter, Backspace, Tab and Esc from other apps: the Popup is open and either the Pen is in range or there is uncommitted Ink. Outside it, the keyboard behaves normally even with the Popup open.

## Preview
The blue rendering of uncommitted Ink shown in the target app, at the Anchor and at its delivered size, together with a caret marking the Anchor. It takes no input, so clicks and Replay pass through it.

**Distinct from:** Ink (what is in the Popup), Replay (which makes it real)

## Undo
Taking back the last stroke in the Popup or, when the Popup has nothing to take back, the last action in the target app (Ctrl+Z). Redo is its counterpart (Ctrl+Y in the app). Express Keys 7/8 and the lower/upper Pen Buttons do these in both modes.

**Distinct from:** Clear (which empties the whole Popup)
