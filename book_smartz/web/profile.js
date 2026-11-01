/* global document, window, HTMLFormElement, HTMLElement, FormData, URLSearchParams, DOMParser, fetch */
const graphemes = new Intl.Segmenter("und", { granularity: "grapheme" });
const trimmed = (value) =>
  value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
function message(form, text) {
  const status = form.querySelector("[data-profile-message]");
  status.textContent = text;
  status.focus({ preventScroll: true });
}
function validate(form) {
  const name = trimmed(form.elements.name.value);
  const biography = form.elements.biography.value.replace(/\r\n?/g, "\n");
  const errors = {
    name: /[\n\r\v\f\u0085\u2028\u2029]/u.test(name)
      ? "Use one line for your display name."
      : [...graphemes.segment(name)].length > 100
        ? "Use 100 visible characters or fewer for your display name."
        : "",
    biography:
      [...graphemes.segment(trimmed(biography) ? biography : "")].length > 500
        ? "Use 500 visible characters or fewer for your biography."
        : "",
  };
  let invalid;
  for (const field of ["name", "biography"]) {
    const control = form.elements[field];
    const error = form.querySelector(`#profile-${field}-error`);
    error.textContent = errors[field];
    error.hidden = !errors[field];
    if (errors[field]) {
      control.setAttribute("aria-invalid", "true");
      invalid ||= control;
    } else control.removeAttribute("aria-invalid");
  }
  if (invalid) {
    message(form, "Check the marked profile fields.");
    invalid.focus();
    return false;
  }
  return true;
}
function checking(form) {
  let evidence = form.querySelector('input[name="checking"]');
  if (!evidence) {
    evidence = document.createElement("input");
    evidence.type = "hidden";
    evidence.name = "checking";
    evidence.value = form.elements.offer.value;
    form.append(evidence);
  }
  if (!form.querySelector('button[value="check"]')) {
    const button = document.createElement("button");
    button.type = "submit";
    button.name = "command";
    button.value = "check";
    button.textContent = "Check your save";
    form.querySelector(".profile-controls").append(button);
  }
  for (const button of form.querySelectorAll("button[name=command]")) {
    button.hidden = button.value === "check" ? false : button.hidden;
    button.disabled = button.value !== "check";
  }
  for (const field of ["name", "biography"])
    form.elements[field].readOnly = true;
  message(
    form,
    "We cannot confirm your profile save yet. Check your save before you try again.",
  );
}
document.addEventListener(
  "submit",
  async (event) => {
    const form = event.target;
    if (
      !(form instanceof HTMLFormElement) ||
      !form.hasAttribute("data-profile-form")
    )
      return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (form.dataset.submitting) return;
    const command = event.submitter?.value || "save";
    if (command === "save" && !validate(form)) return;
    const values = new URLSearchParams(new FormData(form));
    values.set("command", command);
    form.dataset.submitting = "true";
    form.setAttribute("aria-busy", "true");
    for (const button of form.querySelectorAll("button"))
      button.disabled = true;
    for (const field of ["name", "biography"])
      form.elements[field].readOnly = true;
    form.querySelector("[data-profile-message]").textContent =
      command === "save"
        ? "Saving…"
        : command === "check"
          ? "Checking…"
          : "Restoring…";
    try {
      const response = await fetch(form.action, {
        method: "POST",
        headers: {
          "content-type": "application/x-www-form-urlencoded",
          accept: "text/html",
        },
        body: values,
      });
      if (response.status === 401 || response.status === 403) {
        checking(form);
        message(
          form,
          "Sign in to check your profile save. Your entered text remains here.",
        );
        if (response.status === 401) {
          const signIn = document.createElement("a");
          signIn.href = "/auth/google";
          signIn.textContent = "Sign in with Google";
          form.querySelector("[data-profile-message]").append(" ", signIn);
        }
        return;
      }
      if (!response.ok) {
        checking(form);
        return;
      }
      const page = new DOMParser().parseFromString(
        await response.text(),
        "text/html",
      );
      const main = page.querySelector("main");
      if (!main?.querySelector("[data-profile-form]")) {
        checking(form);
        return;
      }
      document.querySelector("main").replaceWith(main);
      document.title = page.title;
      const focus =
        main.querySelector('[aria-invalid="true"]') ||
        main.querySelector("[data-profile-message]");
      if (focus instanceof HTMLElement) focus.focus({ preventScroll: true });
    } catch {
      checking(form);
    } finally {
      if (form.isConnected) {
        delete form.dataset.submitting;
        form.removeAttribute("aria-busy");
        const blocked = Boolean(form.querySelector('input[name="checking"]'));
        for (const button of form.querySelectorAll("button[name=command]"))
          button.disabled = blocked && button.value !== "check";
        for (const field of ["name", "biography"])
          form.elements[field].readOnly = blocked;
      }
    }
  },
  true,
);
// Browser history must reload authoritative values instead of replaying an old form.
window.addEventListener("pageshow", (event) => {
  if (event.persisted) window.location.reload();
});
