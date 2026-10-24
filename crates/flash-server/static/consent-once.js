// The consent page answers once (templates/consent.html, and any page an
// extension puts in its place, which keeps the same form). On a phone the
// hand-off from the redirect back to the assistant's app can leave this
// page on screen for a moment, and a second tap used to replace the first
// tap's redirect, so the assistant never got its code. After the first
// submit every later one is dropped and the buttons go grey. They are
// disabled a tick later, as ui.js does it, because a disabled button is
// left out of the form and the decision travels on the button pressed. A
// page restored from the back-forward cache gets its buttons back.
(function () {
  'use strict';
  const form = document.querySelector('form[action="/oauth/consent"]');
  if (!(form instanceof HTMLFormElement)) return;
  const buttons = Array.from(form.querySelectorAll('button[type="submit"]'));
  let sent = false;

  /** @param {boolean} off */
  function setDisabled(off) {
    for (const button of buttons) {
      if (button instanceof HTMLButtonElement) button.disabled = off;
    }
  }

  form.addEventListener('submit', (event) => {
    if (sent) {
      event.preventDefault();
      return;
    }
    sent = true;
    setTimeout(() => {
      setDisabled(true);
    }, 0);
  });

  window.addEventListener('pageshow', (event) => {
    if (!event.persisted) return;
    sent = false;
    setDisabled(false);
  });
})();
