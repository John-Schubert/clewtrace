<!-- @verifies CT-USE-005 -->
# Themes inspection

Verification method: Inspection. Repeat whenever a color token or component style changes.
Record the result below.

## Checklist

- [ ] First run with the OS in light mode: the app is light. With the OS in dark mode: dark.
- [ ] With the Theme menu on "Follow the OS", change the OS setting while the app is open.
      The app follows without a restart.
- [ ] Pick Light, then Dark, in the Theme menu. The app obeys, ignoring the OS. Restart:
      the choice is kept.
- [ ] Pick "Follow the OS" again. The app follows the OS again.
- [ ] The same three choices work from the command palette (Theme: ...).
- [ ] In both themes, look at every pane, the palette, the editor (text, selection, active
      line, hover tooltip), the problems list, and the tooltip. Nothing is unreadable, and
      no white or black patch shows through.
- [ ] Contrast: `app/src/contrast.test.ts` passes (4.5:1 text, 3:1 focus ring).
- [ ] Only the theme choice is stored in the browser. No project content is.

## Result

Not yet run on a built app.
