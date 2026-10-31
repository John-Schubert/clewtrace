<!-- @verifies CT-USE-004 -->
# App accessibility inspection

Verification method: Inspection, against WCAG 2.1 Level AA. Repeat before each release and
whenever a pane or component changes. Record the result below.

## Checklist

- [ ] Landmarks: `header` (banner), `nav`, `main`, `aside`, and a labelled `section` are
      present, each with an accessible name, and there is a skip link to the main view.
- [ ] Every control is reachable by Tab and has a visible focus ring (3:1 against its
      background in both themes).
- [ ] F6 and Shift+F6 move focus through the four panes in order.
- [ ] Text contrast is at least 4.5:1 in both themes (tokens in `app/src/tokens.css`).
- [ ] The virtual list is one tab stop, moves with the arrow keys, Home, End, and Page keys,
      and exposes `listbox` and `option` roles with position and set size.
- [ ] The command palette traps no focus, closes on Escape, and returns focus to the pane.
- [ ] Errors are announced (`role="alert"`).
- [ ] Nothing depends on color alone.

## Result

Not yet run on a built app. Screen reader pass (NVDA, VoiceOver, Orca) is due at Gate 2.
