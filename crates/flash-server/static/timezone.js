// The browser's time zone, for the server. Every
// `<input data-browser-timezone>` on the page gets it, so a form that
// sets something by the clock can carry it. A tag with `data-adopt` also
// posts it on its own: a page includes that only while the account still
// has the default zone, and the server only ever replaces the default, so
// it runs until it has done its job once.
(function () {
  'use strict';
  const script = document.currentScript;
  let zone;
  try {
    zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return;
  }
  if (zone === '') return;
  for (const field of document.querySelectorAll('input[data-browser-timezone]')) {
    if (field instanceof HTMLInputElement) field.value = zone;
  }
  if (script?.hasAttribute('data-adopt') !== true) return;
  if (zone === 'UTC' || zone === 'Etc/UTC') return;
  fetch('/settings/timezone', {
    method: 'POST',
    credentials: 'same-origin',
    body: new URLSearchParams({ timezone: zone }),
  }).catch(() => {
    // The next page that carries `data-adopt` tries again.
  });
})();
