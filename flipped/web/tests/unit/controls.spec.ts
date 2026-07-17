// @vitest-environment jsdom
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ExaminerControls from "../../app/components/ExaminerControls.vue";

describe("ExaminerControls", () => {
  it("enables only commands allowed by the authoritative state", async () => {
    const wrapper = mount(ExaminerControls, {
      props: { status: "ready", disabled: false },
    });
    const buttons = wrapper.findAll("button");
    expect(buttons[0]?.attributes("disabled")).toBeUndefined();
    expect(buttons[1]?.attributes("disabled")).toBeDefined();
    await buttons[0]?.trigger("click");
    expect(wrapper.emitted("start")).toHaveLength(1);
  });
});
