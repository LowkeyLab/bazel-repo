/* global document, window */
import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import test, { beforeEach } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("", { url: "https://reader.example/profile" });
for (const key of [
  "document",
  "window",
  "HTMLFormElement",
  "HTMLElement",
  "FormData",
  "DOMParser",
])
  globalThis[key] = dom.window[key];
await import("./profile.js");

const editor = (
  status = "Confirmed profile",
) => `<main><p class="profile-name">Saved name</p>
<form action="/profile/edit" data-profile-form>
<input name="csrf" value="csrf-token"><input name="offer" value="original-offer">
<input name="name" aria-describedby="profile-name-error" value="Draft name">
<textarea name="biography">Draft paragraph</textarea>
<p id="profile-name-error" hidden></p><p id="profile-biography-error" hidden></p>
<p data-profile-message tabindex="-1">${status}</p><div class="profile-controls">
<button name="command" value="save">Save changes</button>
<button name="command" value="discard">Discard changes</button>
</div></form></main>`;
const form = () => document.querySelector("form");
const status = () => form().querySelector("[data-profile-message]");
let requests;
let respond;
beforeEach(() => {
  document.body.innerHTML = editor();
  document.title = "Original title";
  requests = [];
  respond = async () => ({
    status: 200,
    ok: true,
    text: async () => `<title>Saved profile</title>${editor("Saved")}`,
  });
  globalThis.fetch = async (url, options) => {
    requests.push({ url, ...options });
    return respond();
  };
});
function dispatch(command = "save", target = form()) {
  const submitter =
    command === null
      ? undefined
      : target.querySelector(`button[value="${command}"]`);
  const event = new window.SubmitEvent("submit", {
    bubbles: true,
    cancelable: true,
    submitter,
  });
  target.dispatchEvent(event);
  return event;
}
async function submit(command = "save") {
  dispatch(command);
  await setImmediate();
}
function assertChecking() {
  assert.equal(form().elements.name.value, "Draft name");
  assert.equal(form().elements.biography.value, "Draft paragraph");
  assert.equal(form().elements.checking.value, "original-offer");
  assert.equal(form().querySelector('button[value="save"]').disabled, true);
  assert.equal(form().querySelector('button[value="discard"]').disabled, true);
  assert.equal(form().querySelector('button[value="check"]').disabled, false);
  assert.equal(form().elements.name.readOnly, true);
  assert.equal(form().elements.biography.readOnly, true);
  assert.equal(form().hasAttribute("aria-busy"), false);
  assert.equal(document.activeElement, status());
}

test("unrelated submit events retain their normal behavior", () => {
  const other = document.createElement("form");
  document.body.append(other);
  assert.equal(dispatch(null, other).defaultPrevented, false);
  const event = new window.Event("submit", { bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  assert.equal(event.defaultPrevented, false);
  assert.equal(requests.length, 0);
});

for (const [field, value, error] of [
  ["name", "First\u0085Second", "Use one line"],
  ["name", "👩‍👩‍👧‍👦".repeat(101), "100 visible characters"],
  ["biography", "👍🏽".repeat(501), "500 visible characters"],
]) {
  test(`validation preserves the ${field} draft and focuses its error`, async () => {
    form().elements[field].value = value;
    await submit();
    assert.equal(requests.length, 0);
    assert.equal(form().elements[field].value, value);
    assert.equal(document.activeElement, form().elements[field]);
    assert.equal(form().elements[field].getAttribute("aria-invalid"), "true");
    assert.match(
      form().querySelector(`#profile-${field}-error`).textContent,
      new RegExp(error),
    );
    assert.match(status().textContent, /Check the marked profile fields/);
  });
}

test("valid grapheme limits and paragraphs reach the POST without changing the draft", async () => {
  const original = form();
  const name = " e\u0301".trim().repeat(100);
  const biography = "👍🏽".repeat(498) + "\n\n";
  original.elements.name.value = name;
  original.elements.biography.value = biography;
  original.elements.name.setAttribute("aria-invalid", "true");
  await submit(null);
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "https://reader.example/profile/edit");
  assert.equal(requests[0].method, "POST");
  assert.equal(
    requests[0].headers["content-type"],
    "application/x-www-form-urlencoded",
  );
  assert.equal(requests[0].body.get("csrf"), "csrf-token");
  assert.equal(requests[0].body.get("offer"), "original-offer");
  assert.equal(requests[0].body.get("command"), "save");
  assert.equal(requests[0].body.get("name"), name);
  assert.equal(requests[0].body.get("biography"), biography);
  assert.equal(original.elements.name.hasAttribute("aria-invalid"), false);
  assert.equal(original.isConnected, false);
  assert.equal(document.title, "Saved profile");
  assert.equal(document.activeElement, status());
});

test("discard bypasses draft validation and focuses a server field error", async () => {
  form().elements.name.value = "x".repeat(101);
  respond = async () => ({
    status: 200,
    ok: true,
    text: async () =>
      editor().replace('name="name"', 'aria-invalid="true" name="name"'),
  });
  await submit("discard");
  assert.equal(requests[0].body.get("command"), "discard");
  assert.equal(document.activeElement, form().elements.name);
});

test("a pending request freezes its values and prevents a duplicate submission", async () => {
  let finish;
  respond = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  dispatch();
  assert.equal(form().getAttribute("aria-busy"), "true");
  assert.equal(status().textContent, "Saving…");
  assert.equal(form().elements.name.readOnly, true);
  assert.equal(form().querySelector('button[value="save"]').disabled, true);
  dispatch();
  assert.equal(requests.length, 1);
  finish({ status: 503, ok: false });
  await setImmediate();
  assertChecking();
});

for (const failure of [
  "network",
  "unauthorized",
  "forbidden",
  "server",
  "wrong-page",
  "body",
]) {
  test(`${failure} retains the draft and requires an explicit check`, async () => {
    respond = async () => {
      if (failure === "network")
        throw new Error("controlled transport failure");
      const code =
        { unauthorized: 401, forbidden: 403, server: 503 }[failure] || 200;
      return {
        status: code,
        ok: code === 200,
        text: async () => {
          if (failure === "body")
            throw new Error("controlled response failure");
          return "<main>Another page</main>";
        },
      };
    };
    await submit();
    assertChecking();
    assert.equal(
      status().querySelectorAll('a[href="/auth/google"]').length,
      failure === "unauthorized" ? 1 : 0,
    );
    assert.match(
      status().textContent,
      failure === "unauthorized" || failure === "forbidden"
        ? /Sign in to check/
        : /cannot confirm/,
    );
    await submit("check");
    assert.equal(requests[1].body.get("command"), "check");
    assert.equal(requests[1].body.get("checking"), "original-offer");
    assert.equal(form().querySelectorAll('input[name="checking"]').length, 1);
    assert.equal(form().querySelectorAll('button[value="check"]').length, 1);
    assertChecking();
  });
}

test("an empty optional biography and a successful original-save check restore the editor", async () => {
  form().elements.biography.value = " \n\t";
  respond = async () => ({ status: 503, ok: false });
  await submit();
  assert.equal(requests.length, 1);
  respond = async () => ({
    status: 200,
    ok: true,
    text: async () => editor("Confirmed original save"),
  });
  dispatch("check");
  assert.equal(status().textContent, "Checking…");
  await setImmediate();
  assert.equal(status().textContent, "Confirmed original save");
  assert.equal(form().elements.name.readOnly, false);
  assert.equal(form().querySelector('button[value="save"]').disabled, false);
});
