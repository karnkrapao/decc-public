// @vitest-environment happy-dom

import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useDismissDetails } from "./use-dismiss-details";

afterEach(cleanup);

function Fixture() {
  useDismissDetails(".dismissable[open]");
  return (
    <div>
      <details className="dismissable">
        <summary>Menu one</summary>
        <button type="button">Inside one</button>
      </details>
      <details className="dismissable">
        <summary>Menu two</summary>
        <button type="button">Inside two</button>
      </details>
      <button type="button">Outside</button>
    </div>
  );
}

describe("useDismissDetails", () => {
  it("keeps the clicked menu open but closes other open menus", () => {
    const { container, getByRole } = render(<Fixture />);
    const details = Array.from(
      container.querySelectorAll<HTMLDetailsElement>(".dismissable"),
    );
    details.forEach((item) => {
      item.open = true;
    });

    fireEvent.mouseDown(getByRole("button", { name: "Inside one" }));

    expect(details[0].open).toBe(true);
    expect(details[1].open).toBe(false);
  });

  it("closes every open menu when clicking outside", () => {
    const { container, getByRole } = render(<Fixture />);
    const details = Array.from(
      container.querySelectorAll<HTMLDetailsElement>(".dismissable"),
    );
    details.forEach((item) => {
      item.open = true;
    });

    fireEvent.mouseDown(getByRole("button", { name: "Outside" }));

    expect(details.every((item) => !item.open)).toBe(true);
  });

  it("closes every open menu on Escape", () => {
    const { container } = render(<Fixture />);
    const details = Array.from(
      container.querySelectorAll<HTMLDetailsElement>(".dismissable"),
    );
    details.forEach((item) => {
      item.open = true;
    });

    fireEvent.keyDown(window, { key: "Escape" });

    expect(details.every((item) => !item.open)).toBe(true);
  });
});
