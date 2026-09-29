---
title: "Rust Is a Mind Virus."
description: "How I became oxidized"
publishDate: 2026-09-29
tags: ["rust", "kotlin", "type-systems"]
draft: true
---

Rust and Kotlin are my two favourite programming languages.

I find both expressive and ergonomic. Kotlin, especially, makes it easy to turn an idea into code that looks reasonably close to what I meant. Its extension functions, null handling, and approach to modelling data make it a language I enjoy using.

So when I say **Rust has infected my brain**, it has some serious competition.

It took about a year for Rust to grow on me. During that year, I learnt some Go, some Haskell, some Zig, and wrote a lot of Kotlin. Eventually I told a friend I was _fully oxidized_.

The strange part is that some of my appreciation for Rust developed while I was writing Kotlin.

I'd be using a language I already considered excellent, with a perfectly reasonable implementation in front of me, and think:

> _Damn. I wish the language could check this part too._

## The compiler has questions

At first, I found Rust hard to get into.

I'd want to do something simple, and the compiler would have questions. **Who owns this value? How long does that reference need to live? What else might be modifying this?**

I understood why the guarantees might be useful. But at the time, it felt like a lot of thinking for a very small amount of code.

So I'd go back to Kotlin. Things would feel comfortable again.

Until I started noticing the questions Rust was asking.

- Can I still use this object after that operation?
- Does my read-only reference point to something another part of the program can change?
- Can I make this library type participate in an abstraction without wrapping it?

Kotlin has good answers to many problems. That's what makes the comparison interesting. Rust still gives me tools I find myself missing.

_And once I notice them, I can't stop noticing them._

## Typestate, but better

Suppose I'm modelling a blog post. **Drafts can be edited and published. Published posts can only be read.**

I can express this nicely in Kotlin with a generic state parameter and extension functions:

```kotlin
class Draft
class Published

class Post<State> internal constructor(
    internal var text: String,
) {
    fun content(): String = text
}

fun draft(content: String): Post<Draft> =
    Post<Draft>(content)

fun Post<Draft>.edit(content: String) {
    text = content
}

fun Post<Draft>.publish(): Post<Published> =
    Post<Published>(text)
```

I keep the constructor and mutable storage internal to the library module. As a caller outside that module, I use the public API:

```kotlin
val post = draft("Rust is a mind virus")
post.edit("Even when you write Kotlin")

val published = post.publish()

published.content()          // Fine
published.edit("Actually…")  // Compile error
```

This is **typestate**: the state is represented in the type, and the available operations follow it. Kotlin checks extension receivers statically, so `edit` doesn't apply to `Post<Published>`. [Kotlin extensions documentation](https://kotlinlang.org/docs/extensions.html)

I can express the same idea in Rust through implementation blocks:

```rust
struct Draft;
struct Published;

struct Post<State> {
    content: String,
    state: State,
}

impl Post<Draft> {
    fn new(content: String) -> Self {
        Self { content, state: Draft }
    }

    fn edit(&mut self, content: String) {
        self.content = content;
    }

    fn publish(self) -> Post<Published> {
        Post {
            content: self.content,
            state: Published,
        }
    }
}

impl<State> Post<State> {
    fn content(&self) -> &str {
        &self.content
    }
}
```

So far, **I've expressed the important restriction in both languages**. The published type doesn't expose editing through this API.

Then I look at what happens to the _original draft_.

In Kotlin:

```kotlin
val post = draft("Rust is a mind virus")
val published = post.publish()

post.edit("One more change")  // Still allowed
val another = post.publish() // Also allowed
```

If publication means creating a snapshot, that's fine. But if I intend publishing to **end the draft's lifecycle**, I haven't expressed that rule.

Rust already has the mechanism:

```rust
fn publish(self) -> Post<Published>
```

Taking `self` by value **consumes the draft**:

```rust
let draft = Post::new("Rust is a mind virus".into());
let published = draft.publish();

draft.edit("One more change".into()); // Compile error: moved
```

I receive a published post and lose access to the consumed draft. For this non-`Copy` type, keeping another variable name doesn't give me an independently usable copy either. [Rust by Example](https://doc.rust-lang.org/rust-by-example/scope/move.html)

Finishing a builder. Committing a transaction. Closing a session. There are plenty of operations after which **I should no longer be able to use the previous value**.

_And so it begins ..._

## Read-only starts feeling insufficient

Kotlin distinguishes read-only collection interfaces from mutable ones:

```kotlin
val names = mutableListOf("Alice", "Bob")
val view: List<String> = names
```

Through `view`, I can't call `add`. But I can still modify the underlying collection through another reference:

```kotlin
names.add("Charlie")

println(view) // [Alice, Bob, Charlie]
```

The read-only interface tells me **what operations this reference exposes**. It doesn't establish that the collection can't change through another reference. [Kotlin collections documentation](https://kotlinlang.org/docs/collections-overview.html)

That's useful. Sometimes it's exactly the contract I want.

But sometimes I want to inspect a collection while knowing that **conflicting mutation can't happen**.

In Rust:

```rust
let mut names = vec![
    String::from("Alice"),
    String::from("Bob"),
];

let view = &names;

names.push(String::from("Charlie")); // Compile error

println!("{view:?}");
```

Because I use `view` after the attempted mutation, its shared borrow must remain valid across that operation. Rust rejects the conflicting mutable access.

If I finish using the view first, mutation becomes available again:

```rust
let view = &names;
println!("{view:?}");

names.push(String::from("Charlie")); // Fine
```

For this ordinary vector, the compiler checks that **shared inspection and exclusive mutation don't overlap**.

In Kotlin, I can take a snapshot, use an [immutable collection](https://github.com/Kotlin/kotlinx.collections.immutable), or coordinate access. I have options.

Rust gives me an extremely efficient option: **lend out access to the existing collection** and have the compiler check when mutation becomes available again.

At first, the borrow checker felt incredibly particular.

Then I'd be reviewing Kotlin code and tracing references to figure out who could change something. I'd start wondering whether I needed a copy, whether an immutable collection would fit better, or whether everyone understood the access convention.

And I'd remember that annoying borrow checker.

_And I'd start missing it ..._

## Extension functions make me want traits

Kotlin's extension functions are one of my favourite features.

I can add a convenient operation to an existing type:

```kotlin
fun String.summary(): String = take(80)

val text = "A rather long article..."
println(text.summary())
```

The call reads naturally. The operation can live in my application without requiring changes to `String`.

But suppose I want **an abstraction that several types can implement**:

```kotlin
interface Summarize {
    fun summary(): String
}

fun renderPreview(value: Summarize): String =
    value.summary()
```

My extension function doesn't make `String` implement that interface:

```kotlin
renderPreview(text) // Compile error
```

Extensions provide callable operations **without changing the class's implemented interfaces**. [Kotlin extensions documentation](https://kotlinlang.org/docs/extensions.html)

I could write an adapter:

```kotlin
class TextSummary(
    private val text: String,
) : Summarize {
    override fun summary(): String = text.take(80)
}

renderPreview(TextSummary(text))
```

But that's not very nice ...

Rust gives me the Haskell (read: ideal) way: **implement my own trait directly for the existing type**.

```rust
trait Summarize {
    fn summary(&self) -> String;
}

impl Summarize for String {
    fn summary(&self) -> String {
        self.chars().take(80).collect()
    }
}

fn render_preview<T: Summarize>(value: &T) -> String {
    value.summary()
}
```

Now I can pass the string directly:

```rust
let text = String::from("A rather long article...");
let preview = render_preview(&text);
```

**I own the trait, so I can implement it for `String`, even though I don't own `String`.** Rust's orphan rules allow this because the trait is defined in my crate, as described in the [Rust Reference](https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules).

This is powerful when I define an abstraction in my application and want existing library types to fit it. I don't need to carry an adapter around just to make the type satisfy my interface.

Now, whenever I write an adapter, a small voice asks whether a trait implementation could have been enough.

_And I'd weep ..._

## Carcinization

None of this changes how ergonomic Kotlin is.

What changed is **my expectation of how much the language can help me reason about a program**.

At first, Rust's questions seemed like extra work.

Then I realized that **I had to answer the same questions anyway**. The only difference was whether I wanted some help with them.

That's when I became fully _oxidized_.

---

**P.S. I haven't even talked about performance.**

Rust's performance gives me room to delay optimization and **focus on shipping features**. Counter-intuitively, a programming language famous for being low-level lets me spend less time thinking about performance, and more time building the product.
