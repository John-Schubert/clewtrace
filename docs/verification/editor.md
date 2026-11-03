<!-- @verifies CT-MOD-004 -->
# Editor inspection

Verification method: Inspection. Repeat whenever the editor, the language support, or the
model browser changes. Record the result below.

## Checklist

- [ ] Open a `.sysml` file in the editor. Keywords, names, strings, numbers, and comments
      have different colors, in both themes.
- [ ] Type the start of a name that exists (such as `Eng`). Completion lists matching
      names with their kinds. Enter inserts one.
- [ ] F12 on a name goes to its definition, opening the file if needed. Ctrl+click does the
      same. A name that leads nowhere does nothing.
- [ ] Shift+F12 lists the places that use the name. Picking one goes there.
- [ ] Hover a name. The tooltip shows the element's kind and name, its doc text if it has
      any (for any kind of element, not only requirements), and its trace status.
- [ ] Add a part in the editor and pause. The model browser shows it without collapsing the
      open packages, and the selected row stays selected.
- [ ] Introduce a syntax error. The problems panel shows it with file, line, and column.
      Fix it: the problem goes.
- [ ] Edit the same file in another program. The editor offers to load the disk version or
      keep your edits.

## Result

Not yet run on a built app.
