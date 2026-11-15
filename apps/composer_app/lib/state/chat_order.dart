/// THE chat order setting. True: the newest item is at the top, directly under the live view, and
/// older ones below it (board E of the phone mockups). False: the conventional order, the newest
/// at the bottom beside the message bar. The chat list reads this and nothing else decides the
/// order, so flipping it needs no change to any widget. False since 2026-11-15: the user wants the newest
/// at the bottom, as every chat client has it.
const chatNewestFirst = false;
