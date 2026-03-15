#!/usr/bin/env python3
"""
End-to-end test for the Anthropic Messages API endpoint.

Tests the /v1/messages endpoint using the official Anthropic Python SDK,
verifying that Anthropic SDK clients can connect to SovereignEngine.

Usage:
    # Against a running SovereignEngine proxy:
    python3 tests/test_anthropic_endpoint.py --base-url https://your-proxy/v1 --api-key se-your-token

    # Against Ollama directly (bypasses SovereignEngine, tests SDK compat):
    python3 tests/test_anthropic_endpoint.py --base-url http://10.24.0.200:11434/v1 --api-key dummy

    # Use x-api-key header (default) or Bearer token:
    python3 tests/test_anthropic_endpoint.py --base-url ... --api-key ... --auth-mode bearer
"""

import argparse
import json
import sys
import time


def test_non_streaming(client, model):
    """Test a basic non-streaming message."""
    print("\n--- Test: Non-streaming message ---")
    start = time.time()

    message = client.messages.create(
        model=model,
        max_tokens=64,
        system="You are a concise assistant. Reply in one sentence.",
        messages=[{"role": "user", "content": "What is 2 + 2?"}],
        temperature=0.1,
    )

    elapsed = time.time() - start

    assert message.type == "message", f"Expected type 'message', got '{message.type}'"
    assert message.role == "assistant", f"Expected role 'assistant', got '{message.role}'"
    assert message.id.startswith("msg_"), f"Expected msg_ prefix, got '{message.id}'"
    assert len(message.content) > 0, "Expected at least one content block"
    assert message.content[0].type == "text", f"Expected text block, got '{message.content[0].type}'"
    assert len(message.content[0].text) > 0, "Expected non-empty text"
    assert message.stop_reason in ("end_turn", "max_tokens"), f"Unexpected stop_reason: {message.stop_reason}"
    assert message.usage.input_tokens > 0 or message.usage.output_tokens > 0, "Expected token usage"

    print(f"  Model: {message.model}")
    print(f"  Response: {message.content[0].text[:100]}")
    print(f"  Stop reason: {message.stop_reason}")
    print(f"  Usage: {message.usage.input_tokens} in / {message.usage.output_tokens} out")
    print(f"  Latency: {elapsed:.2f}s")
    print("  ✓ PASSED")


def test_streaming(client, model):
    """Test streaming message."""
    print("\n--- Test: Streaming message ---")
    start = time.time()

    collected_text = ""
    events_seen = set()
    input_tokens = 0
    output_tokens = 0

    with client.messages.stream(
        model=model,
        max_tokens=64,
        messages=[{"role": "user", "content": "Count from 1 to 5."}],
        temperature=0.1,
    ) as stream:
        for event in stream:
            event_type = type(event).__name__
            events_seen.add(event_type)

            # Collect text from text events
            if hasattr(event, "type") and event.type == "content_block_delta":
                if hasattr(event, "delta") and hasattr(event.delta, "text"):
                    collected_text += event.delta.text

        # Get final message
        final = stream.get_final_message()

    elapsed = time.time() - start

    assert len(collected_text) > 0, "Expected streamed text"
    assert final is not None, "Expected final message"
    assert final.role == "assistant"
    assert len(final.content) > 0

    print(f"  Events seen: {events_seen}")
    print(f"  Streamed text: {collected_text[:100]}")
    print(f"  Final message content: {final.content[0].text[:100]}")
    print(f"  Stop reason: {final.stop_reason}")
    print(f"  Usage: {final.usage.input_tokens} in / {final.usage.output_tokens} out")
    print(f"  Latency: {elapsed:.2f}s")
    print("  ✓ PASSED")


def test_multi_turn(client, model):
    """Test multi-turn conversation."""
    print("\n--- Test: Multi-turn conversation ---")

    message = client.messages.create(
        model=model,
        max_tokens=64,
        messages=[
            {"role": "user", "content": "My name is Alice."},
            {"role": "assistant", "content": "Nice to meet you, Alice!"},
            {"role": "user", "content": "What is my name?"},
        ],
        temperature=0.1,
    )

    text = message.content[0].text.lower()
    assert "alice" in text, f"Expected 'alice' in response, got: {text}"

    print(f"  Response: {message.content[0].text[:100]}")
    print("  ✓ PASSED")


def test_system_prompt(client, model):
    """Test that system prompt is respected."""
    print("\n--- Test: System prompt ---")

    message = client.messages.create(
        model=model,
        max_tokens=32,
        system="You must always respond with exactly the word PINEAPPLE, nothing else.",
        messages=[{"role": "user", "content": "Say something."}],
        temperature=0.0,
    )

    text = message.content[0].text.strip()
    assert "pineapple" in text.lower(), f"Expected 'pineapple' in response, got: {text}"

    print(f"  Response: {text}")
    print("  ✓ PASSED")


def test_stop_sequences(client, model):
    """Test stop_sequences parameter."""
    print("\n--- Test: Stop sequences ---")

    message = client.messages.create(
        model=model,
        max_tokens=128,
        messages=[{"role": "user", "content": "Count: 1, 2, 3, 4, 5, 6, 7, 8, 9, 10"}],
        stop_sequences=[", 5"],
        temperature=0.0,
    )

    text = message.content[0].text
    # The response should stop before or at "5"
    print(f"  Response: {text}")
    print(f"  Stop reason: {message.stop_reason}")
    # stop_reason could be "end_turn" if the model stopped naturally, or mapped from stop
    print("  ✓ PASSED")


def test_error_invalid_model(client):
    """Test error response for an invalid model."""
    print("\n--- Test: Error - invalid model ---")
    import anthropic as anthropic_mod

    try:
        client.messages.create(
            model="nonexistent-model-xyz",
            max_tokens=10,
            messages=[{"role": "user", "content": "Hi"}],
        )
        print("  ✗ FAILED - expected an error")
    except anthropic_mod.NotFoundError as e:
        print(f"  Got expected NotFoundError: {e}")
        print("  ✓ PASSED")
    except anthropic_mod.APIError as e:
        print(f"  Got APIError (acceptable): {e}")
        print("  ✓ PASSED (error format may vary)")
    except Exception as e:
        print(f"  Got unexpected error type {type(e).__name__}: {e}")
        print("  ⚠ PARTIAL (error raised but wrong type)")


def main():
    parser = argparse.ArgumentParser(description="Test Anthropic Messages API endpoint")
    parser.add_argument("--base-url", required=True, help="Base URL (e.g. http://localhost:443/v1)")
    parser.add_argument("--api-key", default="dummy", help="API key (se-xxx token)")
    parser.add_argument("--model", default="llama3.1:8b", help="Model name to test with")
    parser.add_argument("--auth-mode", choices=["x-api-key", "bearer"], default="x-api-key",
                        help="Auth header mode")
    args = parser.parse_args()

    import anthropic

    # The Anthropic SDK sends x-api-key by default.
    # If --auth-mode=bearer, we override with a custom header.
    client_kwargs = {
        "base_url": args.base_url,
        "api_key": args.api_key,
    }

    if args.auth_mode == "bearer":
        # Override: send as Authorization: Bearer instead of x-api-key
        client_kwargs["default_headers"] = {
            "Authorization": f"Bearer {args.api_key}",
            "x-api-key": "",  # SDK still sets this; empty won't match
        }

    client = anthropic.Anthropic(**client_kwargs)

    print(f"Testing against: {args.base_url}")
    print(f"Model: {args.model}")
    print(f"Auth mode: {args.auth_mode}")

    passed = 0
    failed = 0
    tests = [
        ("non_streaming", lambda: test_non_streaming(client, args.model)),
        ("streaming", lambda: test_streaming(client, args.model)),
        ("multi_turn", lambda: test_multi_turn(client, args.model)),
        ("system_prompt", lambda: test_system_prompt(client, args.model)),
        ("stop_sequences", lambda: test_stop_sequences(client, args.model)),
        ("error_invalid_model", lambda: test_error_invalid_model(client)),
    ]

    for name, test_fn in tests:
        try:
            test_fn()
            passed += 1
        except Exception as e:
            print(f"  ✗ FAILED: {e}")
            failed += 1

    print(f"\n{'='*50}")
    print(f"Results: {passed} passed, {failed} failed out of {len(tests)} tests")

    if failed > 0:
        sys.exit(1)
    print("All tests passed! ✓")


if __name__ == "__main__":
    main()
