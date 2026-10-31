<!-- @verifies CT-EFF-011 -->
# Reference machine start time

Verification method: Demonstration, at the Gate 2 check. Repeat before each release and
whenever a change could slow the start. Record the result below.

The reference machine is John's Windows 10 laptop with 4 GB of RAM. The CI runner budget
(CT-EFF-003, 4 s) is a separate Test and does not count here.

## Checklist

- [ ] Install the release build from the installer, not a debug build.
- [ ] Restart the laptop and wait until it is idle, so the start is a cold one.
- [ ] Launch Clewtrace with no project and time it from the launch to a window that
      accepts a click on "Commands" and opens the palette. Use a stopwatch or a screen
      recording with a clock.
- [ ] Repeat three times after a restart each. The slowest is 2 s or less.
- [ ] Record the three times below.

## Result

Not run yet. Record the date, the build, the three times, and who ran it.
