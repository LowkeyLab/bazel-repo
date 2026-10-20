/* global document, window, HTMLFormElement, HTMLButtonElement */
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
  for (const status of form.querySelectorAll("[data-submitting]"))
    status.hidden = false;
});
// Back/forward cache restores the page snapshot; fetch authoritative state again.
window.addEventListener("pageshow", (event) => {
  if (event.persisted) window.location.reload();
});
