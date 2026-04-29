#!/usr/bin/env python3
"""
Daily Tip Server — Full Integration Test Script

Tests every server endpoint including happy paths AND error paths.
Requires: pip install -r requirements-test.txt
Requires: Server running on 127.0.0.1:3001 (cargo run)
"""

import sys
import subprocess
import yaml
import requests

# ─── Compile Protobuf ────────────────────────────────────────
print("Compiling dailytip.proto...")
try:
    subprocess.run(
        [sys.executable, "-m", "grpc_tools.protoc", "-Iproto", "--python_out=.", "proto/dailytip.proto"],
        check=True,
    )
except Exception:
    print("Falling back to protoc...")
    subprocess.run(
        ["protoc", "-I=proto", "--python_out=.", "proto/dailytip.proto"],
        check=True,
    )

import dailytip_pb2

BASE_URL = "http://127.0.0.1:3001"
passed = 0
failed = 0
skipped = 0


def test(name, condition, detail=""):
    global passed, failed
    if condition:
        passed += 1
        print(f"  ✅ {name}")
    else:
        failed += 1
        print(f"  ❌ {name} — {detail}")


def skip(name, detail=""):
    global skipped
    skipped += 1
    print(f"  ⏭️  {name} — SKIPPED ({detail})")


def section(title):
    print(f"\n{'─'*50}")
    print(f"  {title}")
    print(f"{'─'*50}")


def main():
    global passed, failed

    print("╔══════════════════════════════════════════════════╗")
    print("║   Daily Tip Server — Integration Test Suite     ║")
    print("╚══════════════════════════════════════════════════╝")

    # Read admin token
    try:
        with open("settings.yaml", "r") as f:
            settings = yaml.safe_load(f)
            admin_token = settings.get("admin_token", "")
            has_api_key = bool(settings.get("llm_api_key"))
    except FileNotFoundError:
        print("❌ settings.yaml not found. Start the server first (cargo run).")
        sys.exit(1)

    if not admin_token:
        print("❌ admin_token missing from settings.yaml")
        sys.exit(1)

    # ─── Auth Tests ───────────────────────────────────────
    section("AUTH")

    # Wrong token should fail
    resp = requests.post(f"{BASE_URL}/auth/login", json={"admin_token": "wrong_token"})
    test("Login with wrong token → 401", resp.status_code == 401, f"got {resp.status_code}")

    # Correct login
    session = requests.Session()
    resp = session.post(f"{BASE_URL}/auth/login", json={"admin_token": admin_token})
    test("Login with correct token → 200", resp.status_code == 200, f"got {resp.status_code}")

    # Unauthenticated access to admin routes
    anon = requests.Session()
    resp = anon.get(f"{BASE_URL}/admin/settings")
    test("GET /admin/settings without session → 401", resp.status_code == 401, f"got {resp.status_code}")

    resp = anon.get(f"{BASE_URL}/admin/keys")
    test("GET /admin/keys without session → 401", resp.status_code == 401, f"got {resp.status_code}")

    # ─── Admin HTML Page ──────────────────────────────────
    section("ADMIN PAGE")

    resp = requests.get(f"{BASE_URL}/admin")
    test("GET /admin serves HTML → 200", resp.status_code == 200, f"got {resp.status_code}")
    test("Admin HTML contains DAILYTIP", "DAILYTIP" in resp.text, "missing title")
    test("Admin HTML contains login input", "adminTokenInput" in resp.text, "missing input")

    # ─── Settings CRUD ────────────────────────────────────
    section("SETTINGS")

    resp = session.get(f"{BASE_URL}/admin/settings")
    test("GET /admin/settings → 200", resp.status_code == 200, f"got {resp.status_code}")
    data = resp.json()
    test("Settings has model field", "model" in data, f"keys: {list(data.keys())}")
    test("Settings has template field", "template" in data, f"keys: {list(data.keys())}")

    if has_api_key:
        skip("POST /admin/settings (update)", "api_key already set")
        skip("Settings persisted model update", "api_key already set")
        skip("Settings persisted template update", "api_key already set")
    else:
        resp = session.post(f"{BASE_URL}/admin/settings", json={
            "model": "google/gemini-2.5-pro",
            "template": "Tell me about {topic}.",
            "api_key": "",
            "base_url": "https://openrouter.ai/api/v1"
        })
        test("POST /admin/settings (update) → 200", resp.status_code == 200, f"got {resp.status_code}")

        resp = session.get(f"{BASE_URL}/admin/settings")
        data = resp.json()
        test("Settings persisted model update", data["model"] == "google/gemini-2.5-pro", f"got {data.get('model')}")
        test("Settings persisted template update", data["template"] == "Tell me about {topic}.", f"got {data.get('template')}")

    # ─── API Key Management ───────────────────────────────
    section("API KEYS")

    resp = session.post(f"{BASE_URL}/admin/keys", json={"client_name": "python_test_client"})
    test("POST /admin/keys (create) → 200", resp.status_code == 200, f"got {resp.status_code}")
    api_key = resp.json()
    test("Key has sk_live_ prefix", api_key.startswith("sk_live_"), f"got {api_key[:20]}")

    resp = session.get(f"{BASE_URL}/admin/keys")
    test("GET /admin/keys (list) → 200", resp.status_code == 200, f"got {resp.status_code}")
    keys_list = resp.json()
    test("At least 1 key exists", len(keys_list) >= 1, f"got {len(keys_list)}")

    # ─── API Auth Tests ───────────────────────────────────
    section("API AUTH")

    tips_req = dailytip_pb2.TipsQuery(count=1, topics="rust")

    resp = requests.post(f"{BASE_URL}/tips", data=tips_req.SerializeToString())
    test("POST /tips without auth → 401", resp.status_code == 401, f"got {resp.status_code}")

    resp = requests.post(
        f"{BASE_URL}/tips",
        headers={"Authorization": "sk_live_totallyFakeKey123"},
        data=tips_req.SerializeToString(),
    )
    test("POST /tips with invalid key → 401", resp.status_code == 401, f"got {resp.status_code}")

    resp = requests.post(
        f"{BASE_URL}/review",
        headers={"Authorization": "sk_live_fakeKey"},
        data=dailytip_pb2.ReviewPayload(card_id=1, grade=4).SerializeToString(),
    )
    test("POST /review without auth → 401", resp.status_code == 401, f"got {resp.status_code}")

    # ─── Tips Endpoint ────────────────────────────────────
    section("TIPS (Protobuf)")

    tips_req = dailytip_pb2.TipsQuery(count=2, topics="rust, python")
    resp = requests.post(
        f"{BASE_URL}/tips",
        headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
        data=tips_req.SerializeToString(),
    )
    test("POST /tips with valid key → 200", resp.status_code == 200, f"got {resp.status_code}")

    tips_resp = dailytip_pb2.TipsResponse()
    tips_resp.ParseFromString(resp.content)
    test("Got 2 tips back", len(tips_resp.tips) == 2, f"got {len(tips_resp.tips)}")

    if len(tips_resp.tips) >= 1:
        tip = tips_resp.tips[0]
        test("Tip has ID > 0", tip.id > 0, f"got {tip.id}")
        test("Tip has topic", len(tip.topic) > 0, "empty topic")
        test("Tip has full_content", len(tip.full_content) > 0, "empty content")
        test("Tip has compressed_content", len(tip.compressed_content) > 0, "empty compressed")
        test("Tip has topic_class", len(tip.topic_class) > 0, "empty topic_class")
        test("Tip has tipcard_type", tip.tipcard_type in ("srs_tip", "casual_tip", "repeatable_tip"), f"got {tip.tipcard_type}")
        print(f"       └─ [{tip.topic}] {tip.full_content[:50]}...")
    else:
        test("Tip content checks", False, "no tips returned")

    # ─── Review Endpoint ──────────────────────────────────
    section("REVIEW (Protobuf)")

    if len(tips_resp.tips) >= 1:
        card_id = tips_resp.tips[0].id

        review_req = dailytip_pb2.ReviewPayload(card_id=card_id, grade=4)
        resp = requests.post(
            f"{BASE_URL}/review",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=review_req.SerializeToString(),
        )
        test(f"POST /review card {card_id} grade 4 → 200", resp.status_code == 200, f"got {resp.status_code}")

        # Review again with different grade
        review_req = dailytip_pb2.ReviewPayload(card_id=card_id, grade=2)
        resp = requests.post(
            f"{BASE_URL}/review",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=review_req.SerializeToString(),
        )
        test(f"POST /review card {card_id} grade 2 → 200", resp.status_code == 200, f"got {resp.status_code}")

    # Review non-existent card
    ghost = dailytip_pb2.ReviewPayload(card_id=99999, grade=3)
    resp = requests.post(
        f"{BASE_URL}/review",
        headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
        data=ghost.SerializeToString(),
    )
    test("POST /review non-existent card → 404", resp.status_code == 404, f"got {resp.status_code}")

    # ─── Repeatable Cards ─────────────────────────────────
    section("REPEATABLE CARDS")

    repeatable_req = dailytip_pb2.TipsQuery(
        count=1,
        topics="spanish verbs",
        topic_class="re:word",
        tipcard_type="repeatable_tip",
    )
    resp = requests.post(
        f"{BASE_URL}/tips",
        headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
        data=repeatable_req.SerializeToString(),
    )
    test("POST /tips repeatable class → 200", resp.status_code == 200, f"got {resp.status_code}")

    repeatable_resp = dailytip_pb2.TipsResponse()
    repeatable_resp.ParseFromString(resp.content)
    if len(repeatable_resp.tips) >= 1:
        first_id = repeatable_resp.tips[0].id
        test("Repeatable card has expected class", repeatable_resp.tips[0].topic_class == "re:word", f"got {repeatable_resp.tips[0].topic_class}")
        test("Repeatable card has expected type", repeatable_resp.tips[0].tipcard_type == "repeatable_tip", f"got {repeatable_resp.tips[0].tipcard_type}")

        dismiss_req = dailytip_pb2.ReviewPayload(card_id=first_id, action="dismiss")
        resp = requests.post(
            f"{BASE_URL}/review",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=dismiss_req.SerializeToString(),
        )
        test("POST /review repeatable dismiss → 200", resp.status_code == 200, f"got {resp.status_code}")

        resp = requests.post(
            f"{BASE_URL}/tips",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=repeatable_req.SerializeToString(),
        )
        repeatable_next = dailytip_pb2.TipsResponse()
        repeatable_next.ParseFromString(resp.content)
        got_new_card = (
            resp.status_code == 200
            and len(repeatable_next.tips) == 1
            and repeatable_next.tips[0].id != first_id
        )
        test("Dismiss then next /tips gives new card", got_new_card, f"status={resp.status_code}")
    else:
        test("Repeatable card returned", False, "no tips returned")

    # ─── Casual Cards ─────────────────────────────────────
    section("CASUAL CARDS")

    casual_req = dailytip_pb2.TipsQuery(
        count=1,
        topics="rust",
        topic_class="casual",
        tipcard_type="casual_tip",
    )
    resp = requests.post(
        f"{BASE_URL}/tips",
        headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
        data=casual_req.SerializeToString(),
    )
    test("POST /tips casual class → 200", resp.status_code == 200, f"got {resp.status_code}")

    casual_resp = dailytip_pb2.TipsResponse()
    casual_resp.ParseFromString(resp.content)
    if len(casual_resp.tips) >= 1:
        first_id = casual_resp.tips[0].id
        test("Casual card has expected class", casual_resp.tips[0].topic_class == "casual", f"got {casual_resp.tips[0].topic_class}")
        test("Casual card has expected type", casual_resp.tips[0].tipcard_type == "casual_tip", f"got {casual_resp.tips[0].tipcard_type}")

        ack_req = dailytip_pb2.ReviewPayload(card_id=first_id, action="acknowledge")
        resp = requests.post(
            f"{BASE_URL}/review",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=ack_req.SerializeToString(),
        )
        test("POST /review casual acknowledge → 200", resp.status_code == 200, f"got {resp.status_code}")

        resp = requests.post(
            f"{BASE_URL}/tips",
            headers={"Authorization": api_key, "Content-Type": "application/x-protobuf"},
            data=casual_req.SerializeToString(),
        )
        casual_next = dailytip_pb2.TipsResponse()
        casual_next.ParseFromString(resp.content)
        got_new_card = (
            resp.status_code == 200
            and len(casual_next.tips) == 1
            and casual_next.tips[0].id != first_id
        )
        test("Acknowledge then next /tips gives new casual card", got_new_card, f"status={resp.status_code}")
    else:
        test("Casual card returned", False, "no tips returned")

    # ─── Cleanup ──────────────────────────────────────────
    section("CLEANUP")

    key_id_to_delete = None
    for k in keys_list:
        if k["client_name"] == "python_test_client":
            key_id_to_delete = k["id"]
            break

    if key_id_to_delete is not None:
        resp = session.delete(f"{BASE_URL}/admin/keys", json={"id": key_id_to_delete})
        test("DELETE /admin/keys → 200", resp.status_code == 200, f"got {resp.status_code}")

        # Verify deletion
        resp = session.get(f"{BASE_URL}/admin/keys")
        remaining = [k for k in resp.json() if k["client_name"] == "python_test_client"]
        test("Key actually removed from list", len(remaining) == 0, f"still found {len(remaining)}")
    else:
        print("  ⚠️  Could not find test key to delete")

    # ─── Summary ──────────────────────────────────────────
    total = passed + failed + skipped
    print(f"\n{'═'*50}")
    print(f"  Results: {passed}/{total} passed", end="")
    if skipped > 0:
        print(f", {skipped} skipped", end="")
    if failed > 0:
        print(f" — {failed} FAILED ❌")
    else:
        print(" — ALL PASSED ✅")
    print(f"{'═'*50}\n")

    sys.exit(1 if failed > 0 else 0)


if __name__ == "__main__":
    main()
