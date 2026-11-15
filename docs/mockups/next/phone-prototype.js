// Fixture-only behaviour. No server calls; approve, deny, send and site changes stay disabled.
// The markup is already newest first, as the app shows it.
const app = document.querySelector('.p-app');
const transcript = document.querySelector('.p-transcript');
const composer = document.querySelector('.p-composer');
const chatToggle = document.querySelector('[data-chat-toggle]');
const collapse = document.querySelector('[data-live-collapse]');
const expand = document.querySelector('[data-live-expand]');

// Hide chat (proposal): the conversation stays in the page; Show chat restores its scroll.
let savedScroll = null;
function setChatHidden(hidden) {
  if (hidden && !app.hasAttribute('data-chat-hidden')) savedScroll = transcript.scrollTop;
  app.toggleAttribute('data-chat-hidden', hidden);
  transcript.hidden = hidden;
  composer.hidden = hidden;
  chatToggle.textContent = hidden ? 'Show chat' : 'Hide chat';
  chatToggle.setAttribute('aria-expanded', String(!hidden));
  if (!hidden) requestAnimationFrame(() => { transcript.scrollTop = savedScroll ?? 0; });
}
if (chatToggle) {
  transcript.id = 'chat-transcript';
  chatToggle.setAttribute('aria-controls', transcript.id);
  setChatHidden(app.hasAttribute('data-chat-hidden'));
  chatToggle.addEventListener('click', () => setChatHidden(!app.hasAttribute('data-chat-hidden')));
}

// Landscape (proposal): fold the live view into a rail beside the chat, and back. Folding it
// while the chat is hidden brings the chat back: there is always something to read.
function setLiveFolded(folded) {
  if (folded && app.hasAttribute('data-chat-hidden')) setChatHidden(false);
  app.toggleAttribute('data-live-collapsed', folded);
  collapse.setAttribute('aria-expanded', String(!folded));
  expand.setAttribute('aria-expanded', String(!folded));
  (folded ? expand : collapse).focus({ preventScroll: true });
}
collapse?.addEventListener('click', () => setLiveFolded(true));
expand?.addEventListener('click', () => setLiveFolded(false));

// The site sheet: focus starts on the barrier (close) and Tab stays inside the sheet.
const sheet = document.querySelector('.p-sheet');
if (sheet) {
  const close = document.querySelector('.p-scrim');
  // Inventory previews must not steal focus or scroll their parent page.
  if (window === window.top) close.focus({ preventScroll: true });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') location.href = '05-phone-chat.html';
    if (event.key === 'Tab') {
      const links = [close, ...sheet.querySelectorAll('a[href]')];
      const index = links.indexOf(document.activeElement);
      event.preventDefault();
      links[(index + (event.shiftKey ? -1 : 1) + links.length) % links.length].focus();
    }
  });
}
