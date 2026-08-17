# Writing Tests

Tests in this project are based on matklad's [How to Test](https://matklad.github.io/2021/05/31/how-to-test.html).

The goal is not to test every single function in isolation, but make the project easier to change, provide confidence at the boundaries and remain cheap to write and run. A suite that catches every bug but has to be rewritten on every refactor is a mantainability nightmare

This is not a hard requirement and some parts of the code may need a more specific approach, but in general these principles should be applied when possible.

## Principles

### 1. Test features, not code

Tests should describe behaviour that matters to users or callers, rather than the implementation used to provide that behaviour.

Prefer tests for meaningful features through public or stable boundaries, instead of testing private helpers, implementation details or internal call sequences unless those details are part of the behaviour we are testing.

Ask yourself the following question:

> If the implementation was replaced with a completely different implementation that produced the same result, would this test still be useful?

If the answer is no, the test is probably too coupled to the implementation.

#### Example

```rust
 // BAD: tests an implementation detail.
#[test]
fn parser_can_parse_port() {
    let parser = Parser::new("port = 8080");
    assert!(parser.parse_port().is_ok());
}

// GOOD: tests the feature through its public boundary.
#[test]
fn parses_a_valid_config() {
    let config = parse_config("port = 8080").expect("config should be able to be parsed);
    assert_eq!(config.port, 8080);
}
```

### 2. Keep tests resilient to refactoring

Tests are part of the cost of changing code. If in order to modify the implementation, such as adding new params or modifying return types, tens of tests need to be updated, it discourages making improvements.

Prefer a small `check`-style function which calls the API so tests can reuse that code instead. Test cases should primarily consist of input data and expected output. That way, if we change the implementation later, it will only be necessary to update that `check` harness.

```rust
// BAD: couples the test to the implementation.
#[test]
fn greet_uses_string_formatting() {
    assert_eq!(format_greeting("Alice"), "Hello, Alice!");
}

// GOOD: data-driven tests, shared `check-` function
#[track_caller]
fn check_greet(name: &str, expected: &str) {
    assert_eq!(format_greeting(name), expected);
}

#[test]
fn greets_a_user() {
    check_greet("Alice", "Hello, Alice!");
}
```
Always annotate harnesses with `#[track_caller]` so failures point at the test case and not at the assertion inside `check`.

### 3. Make writing new tests cheap

Noone enjoys writing tests, and a test that is difficult to add will eventually not be added. When fixing a bug, adding a regression test should be a small and quick part of the fix.

Invest in the harness once, such as fixture builders, a `check` per feature, shared helpers, etc.

```rust
// A builder keeps the setup out of every test
fn game(title: &str) -> GameBuilder {
    GameBuilder::new().title(title).platform(Platform::Windows)
}

#[test]
fn sorts_library_by_last_played() {
    check_order(
        &[
            game("Hollow Knight").last_played(days_ago(1)),
            game("Celeste").last_played(days_ago(9)),
            game("Hades").never_played(),
        ],
        SortBy::LastPlayed,
        &["Hollow Knight", "Celeste", "Hades"],
    );
}
```

### 4. Keep the core IO-free

IO and sleeps make tests slow, so push them to the caller. A scanner function that takes `Vec<DirEntry>` and returns `Vec<DetectedGame>` is easy to test. One that walks the filesystem is not.

In cases where IO is the whole point, such as game artwork pipepline writing files or launching a process, accept the cost and test it for real in-memory or using `tempfile`.

Tests that are slow should be gated at runtime, never behind a `cfg` feature. That way they still get compiled, and are discoverable and runnable from IDEs.

```rust
#[test]
fn scans_a_large_steam_library() {
    if std::env::var("RUN_SLOW_TESTS").is_err() {
        return;
    }
    // ...
}
```

### 5. Peeking inside

Some behaviour is invisible from the outside. For example, suppose a game cover's accent color can be derived either from its hue or, for grayscale images, from its brightness. The final color alone does not tell us which path was taken. Similarly, when testing a cache, we may want to verify that a value was actually retrieved from the cache rather than recomputed.

The solution is to make these otherwise invisible facts part of the system's observable output. Add explicit observability points, such as a metrics struct or a log line that the test harness can capture and assert on.

Similarly, for "X must not happen" cases, make the code emit coverage marks so tests can assert that the expected branch was taken. This prevents a test from passing for the wrong reason.

```rust
pub fn process_task(img: &[u8]) -> Option<Color> {
    extract_accent(img).0
}

enum AccentSource {
    Hue,
    Monochrome,
}

fn extract_accent(img: &[u8]) -> (Option<Color>, AccentSource) {
    if img.hue() < 0.15 {
        (extract_grayscale(img), AccentSource::Monochrome)
    }
    else {
        (extract_color(img), AccentSource::Hue)
    }
}
```

Tests can now check if the accent color is properly extracted AND if it is being extracted from the proper source.

### 6. Use expect tests when the output is messy

TODO: corresponds to https://matklad.github.io/2021/05/31/how-to-test.html#Expect-Tests.

### 7. Never spawn work you cannot await and never `sleep` in a test

Fire-and-forget tasks make their own tests impossible and leak interference into unrelated tests, because the work outlives the test that started it. Return a handle, a `Task`, a receiver or any other similar mechanism.

If a test contains `sleep`, it is either unreliable or slow, fix the API instead.

```rust
// Untestable: nothing can observe when this finishes.
fn refresh_covers_in_background(library: Library) {
    std::thread::spawn(move || { /* ... */ });
}
```

### 8. Be exhaustive in cheap cases

When a specific function is cheap and the possible input values are small, just check all of them.

## Naming

Test names are sentences about behaviour, not about functions: `launching_a_missing_executable_reports_an_error` instead of `test_launch_err`. If you cannot name it without mentioning a private function, revisit principle 1.
