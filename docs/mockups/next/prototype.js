// Local mockup navigation only: no server state, approvals or real commands.
const search = document.querySelector('.search input');
const options = [...document.querySelectorAll('.option')];
const empty = document.querySelector('.empty');
const selected = () => options.find(option => option.classList.contains('selected'));
function select(option) {
  options.forEach(item => {
    const active = item === option;
    item.classList.toggle('selected', active);
    item.setAttribute('aria-selected', String(active));
  });
  if (search) search.setAttribute('aria-activedescendant', option?.id || '');
}
function filter() {
  const query = search.value.trim().toLowerCase();
  options.forEach(option => { option.hidden = !option.textContent.toLowerCase().includes(query); });
  document.querySelectorAll('.command-group').forEach(group => {
    group.hidden = ![...group.querySelectorAll('.option')].some(option => !option.hidden);
  });
  const visible = options.filter(option => !option.hidden);
  empty.hidden = visible.length > 0;
  select(visible[0]);
  const noun = document.body.classList.contains('screen-sessions') ? 'session' : 'command';
  document.querySelector('[data-count]').textContent = `${visible.length} ${noun}${visible.length === 1 ? '' : 's'}`;
}
search?.addEventListener('input', filter);
select(selected());
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') location.href = '01-workspace.html';
  if (event.ctrlKey && event.shiftKey && ['s', 'p'].includes(event.key.toLowerCase())) {
    event.preventDefault();
    location.href = event.key.toLowerCase() === 's' ? '02-sessions.html' : '03-commands.html';
  }
  if (!search) return;
  const visible = options.filter(option => !option.hidden);
  if (['ArrowDown', 'ArrowUp'].includes(event.key) && visible.length) {
    event.preventDefault();
    const index = visible.indexOf(selected());
    const next = visible[(index + (event.key === 'ArrowDown' ? 1 : -1) + visible.length) % visible.length];
    select(next);
    next.scrollIntoView({ block: 'nearest' });
  }
  if (event.key === 'Enter' && selected()?.dataset.href) {
    event.preventDefault();
    location.href = selected().dataset.href;
  }
});
options.forEach(option => option.addEventListener('click', () => {
  select(option);
  if (option.dataset.href) location.href = option.dataset.href;
}));
