BASE_URL = http://127.0.0.1:8080

# ─── Server ───────────────────────────────────────────────────────────────────

build:
	cargo build

run:
	cargo run

# ─── GET ──────────────────────────────────────────────────────────────────────

test-get-home:
	@echo "\n--- GET / ---"
	curl -i $(BASE_URL)/

test-get-file:
	@echo "\n--- GET /uploads/test.txt ---"
	curl -i $(BASE_URL)/uploads/test.txt

test-get-missing:
	@echo "\n--- GET /does-not-exist (expect 404) ---"
	curl -i $(BASE_URL)/does-not-exist

# ─── POST ─────────────────────────────────────────────────────────────────────

test-post:
	@echo "\n--- POST /uploads/test.txt ---"
	curl -i -X POST $(BASE_URL)/uploads/test.txt \
		--data "hello world"

# ─── DELETE ───────────────────────────────────────────────────────────────────

test-delete:
	@echo "\n--- DELETE /uploads/test.txt ---"
	curl -i -X DELETE $(BASE_URL)/uploads/test.txt

test-delete-missing:
	@echo "\n--- DELETE /does-not-exist (expect 404) ---"
	curl -i -X DELETE $(BASE_URL)/does-not-exist

# ─── Full flow ────────────────────────────────────────────────────────────────

test-all:
	@echo "\n========== 1. Upload file =========="
	curl -i -X POST $(BASE_URL)/uploads/test.txt \
		--data "hello world"

	@echo "\n========== 2. Read it back =========="
	curl -i $(BASE_URL)/uploads/test.txt

	@echo "\n========== 3. Delete it =========="
	curl -i -X DELETE $(BASE_URL)/uploads/test.txt

	@echo "\n========== 4. Confirm it's gone (expect 404) =========="
	curl -i $(BASE_URL)/uploads/test.txt

	@echo "\n========== 5. Bad request =========="
	curl -i --http1.1 -X GARBAGE http://127.0.0.1:8080/

	@echo "\n========== 6. Unknown method (expect 405) =========="
	curl -i -X PATCH $(BASE_URL)/uploads/test.txt

.PHONY: build run \
	test-get-home test-get-file test-get-missing \
	test-post \
	test-delete test-delete-missing \
	test-all


test-login:
	@echo "\n--- Login ---"
	curl -i -X POST http://127.0.0.1:8080/login \
		-c /tmp/cookies.txt \
		--data "username=admin&password=secret"

	@echo "\n--- Who am I? ---"
	curl -i http://127.0.0.1:8080/whoami \
		-b /tmp/cookies.txt

	@echo "\n--- Logout ---"
	curl -i -X POST http://127.0.0.1:8080/logout \
		-b /tmp/cookies.txt \
		-c /tmp/cookies.txt

	@echo "\n--- Who am I after logout? (expect 403) ---"
	curl -i http://127.0.0.1:8080/whoami \
# 		-b /tmp/cookies.txt


test-cgi:
	@echo "\n--- GET CGI script ---"
	curl -i http://127.0.01:8080/cgi/hello.py

	@echo "\n--- POST to CGI script ---"
	curl -i -X POST http://127.0.0.1:8080/cgi/hello.py \
		--data "name=sam&message=hello"

	@echo "\n--- CGI with query string ---"
	curl -i "http://127.0.0.1:8080/cgi/hello.py?foo=bar&baz=qux"

	# ── Phase 4: DELETE and mixed methods ─────────────────────────────────────────

section "Phase 4: DELETE and mixed methods"

# What this tests:
# DELETE correctness — does the file actually disappear?
# Mixed methods — GET + POST + DELETE happening simultaneously
# This is the closest thing to real traffic patterns

# ── DELETE correctness ────────────────────────────────────────────────────────

# Upload a file then delete it
log "POST → DELETE → confirm gone..."
curl -s -o /dev/null -X POST "$BASE/uploads/to_delete.txt" \
    --data "delete me"

STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X DELETE "$BASE/uploads/to_delete.txt")
if [ "$STATUS" = "200" ]; then
    pass "DELETE existing file → 200"
else
    fail "DELETE existing file → expected 200, got $STATUS"
fi

STATUS=$(get_status "$BASE/uploads/to_delete.txt")
if [ "$STATUS" = "404" ]; then
    pass "File gone after DELETE → 404"
else
    fail "File still exists after DELETE (got $STATUS)"
fi

# DELETE missing file
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X DELETE "$BASE/uploads/ghost.txt")
if [ "$STATUS" = "404" ]; then
    pass "DELETE missing file → 404"
else
    fail "DELETE missing file → expected 404, got $STATUS"
fi

# ── Concurrent DELETEs ────────────────────────────────────────────────────────

log "Creating 50 files then deleting concurrently..."

# First upload 50 files sequentially
for i in $(seq 1 50); do
    curl -s -o /dev/null -X POST "$BASE/uploads/del_target_${i}.txt" \
        --data "file $i"
done

# Now delete all 50 concurrently
tmpfile=$(mktemp)
pids=()
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "%{http_code}\n" \
        -X DELETE "$BASE/uploads/del_target_${i}.txt" >> "$tmpfile" &
    pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done

GOT=$(grep -c "^200$" "$tmpfile")
rm -f "$tmpfile"

if [ "$GOT" -eq 50 ]; then
    pass "50 concurrent DELETEs — all 200 ($GOT/50)"
else
    fail "50 concurrent DELETEs — only $GOT/50 succeeded"
fi

# Verify files are actually gone
log "Verifying files are gone from disk..."
STILL_EXISTS=0
for i in $(seq 1 50); do
    if [ -f "www/uploads/del_target_${i}.txt" ]; then
        STILL_EXISTS=$((STILL_EXISTS + 1))
    fi
done
if [ "$STILL_EXISTS" -eq 0 ]; then
    pass "All 50 files confirmed deleted from disk"
else
    fail "$STILL_EXISTS/50 files still on disk after DELETE"
fi

# ── Mixed methods simultaneously ──────────────────────────────────────────────

log "Mixed GET + POST + DELETE simultaneously (150 total requests)..."

tmpfile=$(mktemp)
pids=()

# 50 GETs
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "GET:%{http_code}\n" \
        "$BASE/stress_test/index.html" >> "$tmpfile" &
    pids+=($!)
done

# 50 POSTs
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "POST:%{http_code}\n" \
        -X POST "$BASE/uploads/mixed_${i}.txt" \
        --data "mixed test $i" >> "$tmpfile" &
    pids+=($!)
done

# 50 requests to missing files — expect 404
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "MISS:%{http_code}\n" \
        "$BASE/missing_${i}.txt" >> "$tmpfile" &
    pids+=($!)
done

for pid in "${pids[@]}"; do wait "$pid"; done

# Count results per method
GET_OK=$(grep -c "^GET:200$"  "$tmpfile")
POST_OK=$(grep -c "^POST:200$" "$tmpfile")
MISS_OK=$(grep -c "^MISS:404$" "$tmpfile")
rm -f "$tmpfile"

log "  GET  200: $GET_OK/50"
log "  POST 200: $POST_OK/50"
log "  MISS 404: $MISS_OK/50"

if [ "$GET_OK" -eq 50 ]; then
    pass "Mixed load — 50 GETs all succeeded"
else
    fail "Mixed load — only $GET_OK/50 GETs succeeded"
fi

if [ "$POST_OK" -eq 50 ]; then
    pass "Mixed load — 50 POSTs all succeeded"
else
    fail "Mixed load — only $POST_OK/50 POSTs succeeded"
fi

if [ "$MISS_OK" -eq 50 ]; then
    pass "Mixed load — 50 missing requests all got 404"
else
    fail "Mixed load — only $MISS_OK/50 missing requests got 404"
fi

# Server still alive?
STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server still alive after mixed load"
else
    fail "Server not responding after mixed load (got $STATUS)"
fi

# ── Phase 5: Error handling and bad input under load ─────────────────────────

section "Phase 5: Error handling and bad input"

# What this tests:
# A server that works perfectly with good input means nothing
# if it crashes or hangs on bad input.
# We throw garbage, wrong methods, oversized bodies, and
# malformed requests at it — all concurrently.
# The server must survive every single one.

# ── Wrong methods ─────────────────────────────────────────────────────────────

log "50 concurrent wrong method requests (PATCH) — expect 405..."
tmpfile=$(mktemp)
pids=()
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "%{http_code}\n" \
        -X PATCH "$BASE/stress_test/index.html" >> "$tmpfile" &
    pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done

GOT=$(grep -c "^405$" "$tmpfile")
rm -f "$tmpfile"

if [ "$GOT" -eq 50 ]; then
    pass "50 concurrent PATCH requests → all 405 ($GOT/50)"
else
    fail "50 concurrent PATCH requests → only $GOT/50 got 405"
fi

# ── Garbage requests via netcat ───────────────────────────────────────────────

log "50 garbage requests via netcat — server must survive..."
for i in $(seq 1 50); do
    (printf "GARBAGE JUNK $i\r\n\r\n" | nc -q 1 $HOST $PORT > /dev/null 2>&1) &
done
wait

STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server survived 50 garbage requests"
else
    fail "Server crashed after garbage requests (got $STATUS)"
fi

# ── Empty requests ────────────────────────────────────────────────────────────

log "50 connections that send nothing — testing timeout..."
for i in $(seq 1 50); do
    # Connect but send nothing — server should timeout and close
    (nc -q 1 $HOST $PORT < /dev/null > /dev/null 2>&1) &
done
wait

STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server survived 50 empty connections"
else
    fail "Server crashed after empty connections (got $STATUS)"
fi

# ── Partial requests ──────────────────────────────────────────────────────────

log "20 partial requests — send headers but no body..."
for i in $(seq 1 20); do
    (
        # Send a POST with Content-Length but no body
        printf "POST /uploads/partial.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n" \
        | nc -q 1 $HOST $PORT > /dev/null 2>&1
    ) &
done
wait

STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server survived 20 partial requests"
else
    fail "Server crashed after partial requests (got $STATUS)"
fi

# ── Oversized body ────────────────────────────────────────────────────────────

log "Sending body larger than client_max_body_size — expect 413..."

# Generate a body larger than 20MB limit
# We use a 21MB payload
BIG_BODY=$(python3 -c "print('x' * 21 * 1024 * 1024)")
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/uploads/toobig.txt" \
    --data "$BIG_BODY")

if [ "$STATUS" = "413" ]; then
    pass "Oversized body → 413 Content Too Large"
elif [ "$STATUS" = "200" ]; then
    fail "Oversized body accepted — should have been rejected with 413"
else
    # Some servers close connection on oversized body
    log "  Got status $STATUS (connection may have been closed)"
    pass "Oversized body — server responded without crashing"
fi

# ── Concurrent garbage + valid requests ───────────────────────────────────────

log "Mixing 50 garbage + 50 valid requests simultaneously..."
tmpfile=$(mktemp)
pids=()

# 50 garbage via netcat
for i in $(seq 1 50); do
    (printf "GARBAGE $i\r\n\r\n" | nc -q 1 $HOST $PORT > /dev/null 2>&1) &
done

# 50 valid GETs simultaneously
for i in $(seq 1 50); do
    curl -s -o /dev/null -w "%{http_code}\n" \
        "$BASE/stress_test/index.html" >> "$tmpfile" &
    pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done
wait

GOT=$(grep -c "^200$" "$tmpfile")
rm -f "$tmpfile"

if [ "$GOT" -eq 50 ]; then
    pass "50 valid GETs succeeded while 50 garbage fired simultaneously"
else
    fail "Only $GOT/50 valid GETs succeeded alongside garbage"
fi

# ── Final server health check ─────────────────────────────────────────────────

log "Final health check after all error bombardment..."
STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server fully alive after all error handling tests"
else
    fail "Server not responding after error bombardment (got $STATUS)"
fi


# ── Phase 6: Siege at increasing concurrency ──────────────────────────────────

section "Phase 6: Siege at increasing concurrency"

# What this tests:
# We run siege at increasing concurrency levels — 10, 25, 50, 100, 255
# Each run must maintain 99.5% availability.
# This tells us exactly where (if anywhere) the server starts struggling.
# siege outputs JSON so we use python3 to parse it cleanly.

# Helper — runs siege and checks availability
run_siege() {
    local concurrency="$1"
    local duration="$2"
    local url="$3"
    local label="$4"

    log "Siege: $label (${duration}s, ${concurrency} concurrent)..."

    # Capture siege output — it prints to stderr so redirect 2>&1
    local output
    output=$(siege -b -t "${duration}s" -c "$concurrency" \
        --log=/dev/null "$url" 2>&1)

    # Parse availability from JSON using python3
    local availability
    availability=$(echo "$output" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data['availability'])
except:
    print('0')
")

    local transactions
    transactions=$(echo "$output" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data['transactions'])
except:
    print('0')
")

    local rate
    rate=$(echo "$output" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data['transaction_rate'])
except:
    print('0')
")

    local failed
    failed=$(echo "$output" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    print(data['failed_transactions'])
except:
    print('0')
")

    log "  Availability:    ${availability}%"
    log "  Transactions:    ${transactions}"
    log "  Rate:            ${rate} req/s"
    log "  Failed:          ${failed}"

    # Check availability >= 99.5
    local ok
    ok=$(python3 -c "print('yes' if float('${availability}') >= 99.5 else 'no')")

    if [ "$ok" = "yes" ]; then
        pass "$label — availability ${availability}% (${transactions} tx, ${rate} req/s)"
    else
        fail "$label — availability ${availability}% (required >= 99.5%)"
    fi
}

# Warmup
run_siege 10  10 "$BASE/stress_test/index.html" "10 concurrent users"

# Ramp up
run_siege 25  15 "$BASE/stress_test/index.html" "25 concurrent users"
run_siege 50  20 "$BASE/stress_test/index.html" "25 concurrent users"
run_siege 100 20 "$BASE/stress_test/index.html" "100 concurrent users"

# Large files under concurrency
run_siege 50  15 "$BASE/stress_test/100kb.txt"  "50 concurrent — 100KB files"

# Maximum concurrency — this is the real test
run_siege 255 30 "$BASE/stress_test/index.html" "255 concurrent users"

# Sustained load
run_siege 100 60 "$BASE/stress_test/index.html" "100 concurrent — 60s sustained"

# Server still alive after all siege runs?
STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server alive after all siege runs"
else
    fail "Server not responding after siege (got $STATUS)"
fi


# ── Phase 7: Cleanup ──────────────────────────────────────────────────────────

section "Phase 7: Cleanup"

# What this does:
# Removes all test files created during the stress test.
# A clean server is a happy server.

log "Removing stress test files..."
rm -rf www/stress_test
rm -rf www/uploads/phase3_*
rm -rf www/uploads/mixed_*
rm -rf www/uploads/del_target_*
rm -rf www/uploads/roundtrip.txt
rm -rf www/uploads/to_delete.txt
rm -rf www/uploads/siege_test.txt
rm -rf www/uploads/large_*
rm -rf www/uploads/toobig.txt
rm -rf www/uploads/empty.txt
rm -rf www/uploads/partial.txt
rm -f /tmp/siege_urls.txt
pass "Test files cleaned up"

# Final server health check — after everything
STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "404" ]; then
    pass "Cleanup verified — test files gone"
else
    fail "Cleanup failed — files still present (got $STATUS)"
fi

STATUS=$(get_status "$BASE/")
if [ "$STATUS" = "200" ] || [ "$STATUS" = "404" ];








