// Inventory thumbnails are the actual pages, not reference images or stale renders.
document.querySelectorAll('.thumbnail').forEach(box => {
  const frame = box.querySelector('iframe');
  const width = Number(frame.dataset.width || 2000);
  const height = Number(frame.dataset.height || 1246);
  frame.style.width = `${width}px`;
  frame.style.height = `${height}px`;
  const resize = () => {
    const scale = frame.dataset.fit === 'contain'
      ? Math.min(box.clientWidth / width, box.clientHeight / height)
      : box.clientWidth / width;
    frame.style.transform = `scale(${scale})`;
    if (frame.dataset.fit === 'contain') frame.style.left = `${(box.clientWidth - width * scale) / 2}px`;
  };
  resize();
  new ResizeObserver(resize).observe(box);
});
