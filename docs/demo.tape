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
Sleep 3s
Show

# Today view: grouped by urgency, with counts in the header
Type "jobi" Enter
Sleep 2.5s
Down
Sleep 1.2s

# Search with a filter, then review one application end to end
Type "/"
Type "north"
Enter
Sleep 1s
Type "v"
Sleep 3s
Type " "
Sleep 1.2s
Escape
Sleep 500ms
Escape
Sleep 500ms

# Move it along the pipeline, then undo
Type "1"
Sleep 600ms
Down
Sleep 400ms
Type "s"
Sleep 1.8s
Type "u"
Sleep 1.5s

# Add a note and a new application with a friendly date
Type "n"
Type "Panel confirmed, sent diagrams"
Enter
Sleep 1.2s
Type "a"
Sleep 600ms
Type "Orbit Labs"
Tab
Type "Data Engineer"
Tab
Tab
Type "Send resume"
Tab
Type "fri"
Sleep 800ms
Enter
Sleep 2s

# Pipeline statistics
Type "t"
Sleep 3s
Type " "
Sleep 400ms
Type "q"
Sleep 600ms

Type "jobi status"
Enter
Sleep 1.5s
Type "jobi offers"
Enter
Sleep 2.5s
