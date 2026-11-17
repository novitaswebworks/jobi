#!/bin/sh
# Creates a throwaway demo database with dates relative to today.
# Usage: JOBI_DB=/tmp/jobi-demo.sqlite3 sh docs/demo-seed.sh
set -e
d() { python3 -c "import datetime,sys; print(datetime.date.today()+datetime.timedelta(days=int(sys.argv[1])))" "$1"; }
rm -f "$JOBI_DB"
jobi add --company "Acme Robotics" --role "Backend Engineer" --stage Applied --action "Follow up with recruiter" --due "$(d -1)"
jobi add --company "Northwind" --role "Platform Engineer" --stage Interview --action "Prepare system design round" --due "$(d 0)"
jobi add --company "Lumen Labs" --role "Rust Developer" --stage Screening --action "Send portfolio link" --due "$(d 3)"
jobi add --company "Fjord Systems" --role "Site Reliability Engineer" --stage Wishlist
jobi posting 2 --url https://example.com/careers/platform-engineer --description "Own the deploy platform. Rust and Kubernetes."
jobi interview 2 --at "$(d 2)T15:00:00+05:30" --timezone Asia/Kolkata --kind "System design" --notes "Bring architecture diagrams"
jobi note 2 "Recruiter confirmed a 60 minute panel"
