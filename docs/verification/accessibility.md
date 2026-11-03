<!-- @verifies CT-USE-004, CT-USE-010 -->
# Accessibility inspection

Verification method: Inspection, against WCAG 2.1 Level AA. Repeat before each release and
whenever a pane or component changes. Record the result below.

## Checklist

- [ ] Landmarks: `header` (banner), `nav`, `main`, `aside`, and a labelled `section` are
      present, each with an accessible name, and there is a skip link to the main view.
- [ ] Every control is reachable by Tab and has a visible focus ring (3:1 against its
      background in both themes).
- [ ] F6 and Shift+F6 move focus through the four panes in order: model browser, main view,
      trace panel, problems.
- [ ] Keyboard paths: in each pane, every action can be done without a mouse. Check the
      model browser (arrows, Enter), the main view (view buttons, detail, coverage, search,
      source, editor), the trace panel (back, forward, tabs, lists), and the problems list.
- [ ] Text contrast is at least 4.5:1 in both themes (tokens in `app/src/lib/Tokens.svelte`).
      A unit test (`app/src/contrast.test.ts`) checks every text pair and the focus ring.
- [ ] The virtual list is one tab stop, moves with the arrow keys, Home, End, and Page keys,
      and exposes `listbox` and `option` roles with position and set size.
- [ ] The command palette traps no focus, closes on Escape, and returns focus to the pane.
- [ ] Errors are announced (`role="alert"`).
- [ ] Nothing depends on color alone.
- [ ] Tooltips (CT-USE-010): hover a control and the tooltip shows its name and shortcut.
      Tab to a control and the same tooltip shows. Esc closes it. No control has a `title`
      attribute (a unit test checks this).

## Screen readers

Checked by John by hand. Steps are written for a person who has not used the reader before.

### Narrator on Windows 10 or 11

Start Narrator with Win+Ctrl+Enter. Stop it with the same keys. The Narrator key is Caps Lock
or Insert.

- [ ] Open Clewtrace. Narrator reads the window title. Press Tab once: it reads "Skip to
      main view". Press Enter: focus moves to the main view and it reads "Main view".
- [ ] Press F6 four times. Narrator reads each pane by name: "Model browser", "Main view",
      "Trace panel", "Problems".
- [ ] Open a project folder. In the model browser, press Down arrow. Narrator reads each
      element with its kind and name, and whether it is expanded or collapsed. Press Right
      arrow on a package: it reads "expanded".
- [ ] In the main view, Tab through the view buttons (Detail, Coverage, Search, Source,
      Editor). Narrator reads each name and whether it is pressed.
- [ ] Hover the pointer on a button, then Tab to another. Narrator reads the button name and
      then its tooltip as the description (name and shortcut). Press Esc: the tooltip closes.
- [ ] Open the command palette with Ctrl+Shift+P. Narrator reads "Command palette, dialog"
      and then "Type a command". Type "theme" and press Down: it reads each match and its
      position. Press Esc: focus returns to the pane you left.
- [ ] Cause an error (save a `.sysml` file with a syntax error). Narrator reads the problem
      without you moving focus.
- [ ] In the editor, Narrator reads the text by line. Press F12 on a name: it moves to the
      definition and reads the new line.
- [ ] Anything Narrator reads as "unlabeled", "button" with no name, or "group" with no name
      is a failure. Write the control and pane in the Result.

### VoiceOver on macOS

- [ ] Not checked by hand. The maintainer has no Mac. The same steps as for Narrator apply
      (start VoiceOver with Cmd+F5). Until someone with a Mac runs them, macOS screen
      reader support is unverified. Do not claim it.

## Result

Not yet run on a built app. Narrator is due at Gate 2. VoiceOver: not checked by hand.
The contrast, tooltip, and no-`title` checks run in `npm test`.
