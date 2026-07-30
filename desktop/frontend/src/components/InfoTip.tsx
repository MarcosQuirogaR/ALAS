// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useCallback, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { useHelp } from "../lib/helpContext";

// Hover-help for a single variable/option.
//
// There is deliberately NO separate ⓘ button: the *label itself* is the hover
// target, so reading about a variable costs nothing but pointing at it and the
// forms stay visually clean (an icon per row read as clutter). The popover is
// rendered via createPortal to document.body -- the same escape-the-overflow
// trick FigureCard's modal uses -- so it is never clipped by a scroll container
// or a card's overflow, and it's positioned next to the hovered label.
//
// Renders its children unwrapped when View > Learn-more help is off (see
// helpContext.tsx), which is the in-app escape hatch for the whole help layer.

type HelpHoverProps = {
  /** The explanation shown on hover/focus. */
  content: ReactNode;
  /** Bold heading atop the popover (usually the variable's label). */
  heading?: string;
  /** The visible element that acts as the hover target (e.g. the label text). */
  children: ReactNode;
  /** Max popover width in px (default 320). */
  width?: number;
  /** Extra class names for the wrapper span. */
  className?: string;
};

export function HelpHover({ content, heading, children, width = 320, className }: HelpHoverProps) {
  const { verbose } = useHelp();
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number; flip: boolean } | null>(null);
  const hostRef = useRef<HTMLSpanElement>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const openTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const place = useCallback(() => {
    const el = hostRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // Flip above when there isn't comfortable room below.
    const flip = window.innerHeight - r.bottom < 190;
    const left = Math.min(Math.max(8, r.left), window.innerWidth - width - 8);
    setPos({ top: flip ? r.top : r.bottom, left, flip });
    setOpen(true);
  }, [width]);

  // Small open delay: the label is a large hover target that the cursor often
  // just passes over on its way to the input, and popping a panel on every
  // transit would be noise rather than help.
  const show = useCallback(() => {
    if (closeTimer.current) {
      clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    if (openTimer.current || open) return;
    openTimer.current = setTimeout(() => {
      openTimer.current = null;
      place();
    }, 260);
  }, [open, place]);

  const hide = useCallback(() => {
    if (openTimer.current) {
      clearTimeout(openTimer.current);
      openTimer.current = null;
    }
    closeTimer.current = setTimeout(() => setOpen(false), 80);
  }, []);

  if (!verbose || content == null || content === "") {
    return <span className={className}>{children}</span>;
  }

  return (
    <span
      ref={hostRef}
      className={(className ? className + " " : "") + "af-help-target"}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={place}
      onBlur={hide}
      tabIndex={0}
    >
      {children}
      {open &&
        pos &&
        createPortal(
          <div
            className="af-infotip-pop"
            role="tooltip"
            style={{
              position: "fixed",
              left: pos.left,
              top: pos.flip ? undefined : pos.top + 6,
              bottom: pos.flip ? window.innerHeight - pos.top + 6 : undefined,
              width,
            }}
          >
            {heading && <div className="af-infotip-head">{heading}</div>}
            <div className="af-infotip-body">{content}</div>
          </div>,
          document.body
        )}
    </span>
  );
}
