<!-- @verifies CT-USE-002 -->
# Command palette inspection

Verification method: Inspection. Repeat whenever a button, menu, or shortcut is added.
Record the result below.

Rule: every action the interface offers is also in the palette (Ctrl+Shift+P, or Ctrl+K).

## Checklist

- [ ] Ctrl+Shift+P and Ctrl+K open the palette. Escape closes it and focus returns to the
      pane you were in.
- [ ] Typing filters by every word, in any order. Up and Down move; Enter runs.
- [ ] Each action below is in the palette, and running it does what the button does:
  - [ ] Open project folder...
  - [ ] Clear recent projects
  - [ ] Show detail, Show coverage, Search the model, Show source, Show editor
  - [ ] Coverage: each gap filter (one entry per filter)
  - [ ] Trace: back, Trace: forward, Trace: show links, Trace: show impact (the trace panel
        tab changes)
  - [ ] Editor: save the file, Editor: undo, Editor: redo (with a file open in the editor;
        with none open they do nothing)
  - [ ] Theme: follow the OS setting, Theme: light, Theme: dark
  - [ ] Go to Model browser, Main view, Trace panel, Problems
  - [ ] Show all commands
- [ ] Add up the buttons on screen. Each one has an entry above.

## Result

Not yet run on a built app.
