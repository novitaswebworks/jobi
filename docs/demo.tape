# Re-record with: vhs docs/demo.tape   (needs vhs, ttyd, ffmpeg and `jobi` on PATH)
Output docs/demo.gif
Output docs/demo.mp4
Set Shell "bash"
Set FontSize 17
Set Width 1280
Set Height 760
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

Hide
Type "export JOBI_DB=$(mktemp -d)/demo.sqlite3 && sh docs/demo-seed.sh > /dev/null && clear" Enter
Sleep 2s
Show

Type "jobi" Enter
Sleep 2.5s
Down
Sleep 1s
Down
Sleep 1s
Up
Sleep 800ms

# Search, then inspect an application's interview prep
Type "/"
Type "north"
Enter
Sleep 1.2s
Type "v"
Sleep 3s
Escape
Sleep 500ms
Escape
Sleep 500ms

# Add a note, complete the action
Type "1"
Sleep 800ms
Down
Sleep 500ms
Type "n"
Type "Panel confirmed, sent diagrams"
Enter
Sleep 1.5s

# Add a new application
Type "a"
Sleep 800ms
Type "Orbit Labs"
Tab
Type "Data Engineer"
Tab
Right
Right
Tab
Type "Send resume"
Tab
Type "2026-12-01"
Sleep 1s
Enter
Sleep 2s

Type "2"
Sleep 1.5s
Type "?"
Sleep 3s
Type " "
Sleep 500ms
Type "q"
Sleep 800ms

Type "jobi today"
Enter
Sleep 2.5s
