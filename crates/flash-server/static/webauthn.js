// WebAuthn ceremony glue: converts between the server's JSON challenge
// format (base64url) and the browser's ArrayBuffer credential API.
(function () {
  "use strict";

  /**
   * @param {string} id
   * @param {Record<string, string | number>} [vars]
   * @returns {string}
   */
  const tx = (id, vars) => (window.flashT === undefined ? id : window.flashT(id, vars));

  /** @param {string} s */
  function b64uToBuf(s) {
    const b64 = s.replace(/-/g, "+").replace(/_/g, "/");
    const pad = b64.length % 4 === 0 ? "" : "=".repeat(4 - (b64.length % 4));
    return Uint8Array.from(atob(b64 + pad), (c) => c.charCodeAt(0)).buffer;
  }

  /** @param {ArrayBuffer} buf */
  function bufToB64u(buf) {
    let bin = "";
    for (const b of new Uint8Array(buf)) bin += String.fromCharCode(b);
    return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  const { isRecord } = window.flashDom;

  /**
   * `value[key]` as a string; throws naming the key when it is not one.
   * @param {Record<string, unknown>} value
   * @param {string} key
   */
  function text(value, key) {
    const got = value[key];
    if (typeof got !== "string") throw new Error(`The server's reply has no ${key}.`);
    return got;
  }

  /**
   * `value[key]` as an object; throws naming the key when it is not one.
   * @param {unknown} value
   * @param {string} key
   */
  function object(value, key) {
    const got = isRecord(value) ? value[key] : undefined;
    if (!isRecord(got)) throw new Error(`The server's reply has no ${key}.`);
    return got;
  }

  /**
   * A credential list (excludeCredentials, allowCredentials) with each id
   * as bytes. Absent stays absent.
   * @param {unknown} list
   * @returns {PublicKeyCredentialDescriptor[] | undefined}
   */
  function descriptors(list) {
    if (list === undefined || list === null) return undefined;
    if (!Array.isArray(list)) throw new Error(tx("js-passkey-bad-list"));
    return list.map((/** @type {unknown} */ c) => {
      if (!isRecord(c)) throw new Error(tx("js-passkey-bad-entry"));
      if (c["type"] !== "public-key") throw new Error(tx("js-passkey-not-passkey"));
      /** @type {PublicKeyCredentialDescriptor} */
      const out = { type: "public-key", id: b64uToBuf(text(c, "id")) };
      const transports = c["transports"];
      if (Array.isArray(transports) && transports.every((t) => typeof t === "string")) {
        // The browser rejects a transport it does not know.
        out.transports = /** @type {AuthenticatorTransport[]} */ (transports);
      }
      return out;
    });
  }

  /**
   * @param {string} url
   * @param {unknown} [body]
   * @returns {Promise<unknown>}
   */
  async function post(url, body) {
    const res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  /** @param {unknown} err */
  function showError(err) {
    const el = document.getElementById("error");
    if (el === null) return;
    if (err instanceof DOMException && err.name === "NotAllowedError") el.textContent = tx("js-passkey-cancelled");
    else if (err instanceof Error) el.textContent = err.message;
    else el.textContent = tx("js-passkey-failed");
  }

  /**
   * Where to go after signing in: the server's word when it gives one.
   * @param {unknown} reply
   * @param {string} fallback
   */
  function nextFrom(reply, fallback) {
    const next = isRecord(reply) ? reply["next"] : undefined;
    return typeof next === "string" && next !== "" ? next : fallback;
  }

  /**
   * @param {string} token
   * @param {string} next
   */
  async function enroll(token, next) {
    const email = document.getElementById("email");
    if (email instanceof HTMLInputElement && !email.reportValidity()) return;
    const challenge = await post(`/auth/enroll/${token}/start`, {
      email: email instanceof HTMLInputElement ? email.value : "",
      next,
    });
    const pk = object(challenge, "publicKey");
    const user = object(pk, "user");
    const exclude = descriptors(pk["excludeCredentials"]);
    // The fields that travel as base64url are checked and turned into
    // bytes here; the rest is the server's dictionary as it sent it, and
    // the browser checks that against the spec before it does anything.
    const publicKey = /** @type {PublicKeyCredentialCreationOptions} */ (
      /** @type {unknown} */ ({
        ...pk,
        challenge: b64uToBuf(text(pk, "challenge")),
        user: { ...user, id: b64uToBuf(text(user, "id")) },
        ...(exclude === undefined ? {} : { excludeCredentials: exclude }),
      })
    );
    const cred = await navigator.credentials.create({ publicKey });
    if (!(cred instanceof PublicKeyCredential) || !(cred.response instanceof AuthenticatorAttestationResponse)) {
      throw new Error(tx("js-passkey-none-made"));
    }
    const out = await post("/auth/enroll/finish", {
      id: cred.id,
      rawId: bufToB64u(cred.rawId),
      type: cred.type,
      extensions: cred.getClientExtensionResults(),
      response: {
        attestationObject: bufToB64u(cred.response.attestationObject),
        clientDataJSON: bufToB64u(cred.response.clientDataJSON),
      },
    });
    window.location.href = nextFrom(out, "/");
  }

  /** @param {string} next */
  async function login(next) {
    const challenge = await post("/auth/login/start");
    const pk = object(challenge, "publicKey");
    const allow = descriptors(pk["allowCredentials"]);
    // As for enrolling: what is converted is checked, the browser checks
    // the rest.
    const publicKey = /** @type {PublicKeyCredentialRequestOptions} */ (
      /** @type {unknown} */ ({
        ...pk,
        challenge: b64uToBuf(text(pk, "challenge")),
        ...(allow === undefined ? {} : { allowCredentials: allow }),
      })
    );
    /** @type {Credential | null} */
    let cred;
    try {
      cred = await navigator.credentials.get({ publicKey });
    } catch (err) {
      // The prompt ended with no passkey: cancelled, or none on this
      // device for this site (one a security key made before 0.4.0 is
      // not offered any more).
      if (err instanceof DOMException && (err.name === "NotAllowedError" || err.name === "AbortError")) {
        throw new Error(tx("js-passkey-not-used"));
      }
      throw err;
    }
    if (!(cred instanceof PublicKeyCredential) || !(cred.response instanceof AuthenticatorAssertionResponse)) {
      throw new Error(tx("js-passkey-none-used"));
    }
    await post("/auth/login/finish", {
      id: cred.id,
      rawId: bufToB64u(cred.rawId),
      type: cred.type,
      extensions: cred.getClientExtensionResults(),
      response: {
        authenticatorData: bufToB64u(cred.response.authenticatorData),
        clientDataJSON: bufToB64u(cred.response.clientDataJSON),
        signature: bufToB64u(cred.response.signature),
        userHandle: cred.response.userHandle === null ? null : bufToB64u(cred.response.userHandle),
      },
    });
    window.location.href = next;
  }

  /**
   * The page's own `data-next`, or the dashboard.
   * @param {HTMLElement} button
   */
  function nextOf(button) {
    const next = button.dataset["next"];
    return next === undefined || next === "" ? "/" : next;
  }

  // Buttons are wired here (not inline handlers) because the CSP forbids
  // inline script.
  document.addEventListener("DOMContentLoaded", () => {
    const enrollButton = document.getElementById("enroll-btn");
    if (enrollButton !== null) {
      enrollButton.addEventListener("click", () => {
        const token = enrollButton.dataset["token"];
        if (token === undefined || token === "") {
          showError(new Error(tx("js-passkey-link-incomplete")));
          return;
        }
        enroll(token, nextOf(enrollButton)).catch(showError);
      });
    }
    const loginButton = document.getElementById("login-btn");
    if (loginButton !== null) {
      loginButton.addEventListener("click", () => {
        login(nextOf(loginButton)).catch(showError);
      });
    }
  });
})();
