/* global document, window, HTMLFormElement, HTMLButtonElement, HTMLElement */
const pointerKey = "book-smartz-pointer-action";
let pointerButton;
document.addEventListener("pointerdown", (event) => {
  pointerButton = event.target.closest?.("button[type=submit]");
});
document.addEventListener("keydown", () => {
  pointerButton = undefined;
});
function pointerAction(value) {
  try {
    const previous = window.sessionStorage.getItem(pointerKey);
    window.sessionStorage.removeItem(pointerKey);
    if (value) window.sessionStorage.setItem(pointerKey, value);
    return previous;
  } catch {
    return null;
  }
}
const previousPointerAction = pointerAction();
// Keep the selected preference in native form data before disabling controls.
document.addEventListener("submit", (event) => {
  const form = event.target;
  if (
    !(form instanceof HTMLFormElement) ||
    !form.hasAttribute("data-ranking-action")
  )
    return;
  if (form.dataset.submitting) {
    event.preventDefault();
    return;
  }
  const action = form.querySelector('input[name="action"]')?.value;
  pointerAction(event.submitter === pointerButton ? action : null);
  pointerButton = undefined;
  form.dataset.submitting = "true";
  form.setAttribute("aria-busy", "true");
  const button = event.submitter;
  if (button instanceof HTMLButtonElement && button.name) {
    const choice = document.createElement("input");
    choice.type = "hidden";
    choice.name = button.name;
    choice.value = button.value;
    form.append(choice);
  }
  for (const control of form.querySelectorAll("button"))
    control.disabled = true;
  for (const status of form.querySelectorAll("[data-submitting]")) {
    status.hidden = false;
    status.textContent = "Saving…";
  }
});
// Only a fresh confirmed server receipt can make a row eligible for decoration.
const completedRow = [
  ...document.querySelectorAll("ol li[data-completion-action]"),
].find((row) => row.dataset.completionAction === previousPointerAction);
if (completedRow) completedRow.classList.add("ranked-highlight");
function clearHighlight() {
  completedRow?.classList.remove("ranked-highlight");
}
window.addEventListener("pagehide", clearHighlight);
// Back/forward cache restores the page snapshot. Read authoritative state again.
window.addEventListener("pageshow", (event) => {
  if (event.persisted) {
    clearHighlight();
    window.location.reload();
  }
});

// Move response focus only after navigation receives a rendered server response.
const responseFocus =
  document.querySelector(
    '[data-response-focus="error"], [data-response-focus="recovery"]',
  ) ||
  document.querySelector('[data-response-focus="result"]') ||
  document.querySelector("[data-comparison-heading]") ||
  document.querySelector('[data-response-focus="task"]');
if (responseFocus instanceof HTMLElement)
  responseFocus.focus({ preventScroll: true });
