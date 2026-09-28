import { useEffect } from "react";

export function useDismissDetails(selector: string) {
  useEffect(() => {
    function closeOutside(event: MouseEvent) {
      const target = event.target;
      if (!(target instanceof Node)) return;

      document
        .querySelectorAll<HTMLDetailsElement>(selector)
        .forEach((details) => {
          if (!details.contains(target)) details.removeAttribute("open");
        });
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      document
        .querySelectorAll<HTMLDetailsElement>(selector)
        .forEach((details) => details.removeAttribute("open"));
    }

    window.addEventListener("mousedown", closeOutside);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("mousedown", closeOutside);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [selector]);
}
