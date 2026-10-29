---
title: "Rust Is a Mind Virus."
description: "How I became oxidized"
publishDate: 2026-10-07
tags: ["rust", "kotlin", "type-systems"]
type: essay
draft: false
---

<!-- Personal essays intentionally use first-person narration. -->
<!-- vale Google.FirstPerson = NO -->
<!-- vale Google.We = NO -->

Rust and Kotlin are my two favourite programming languages.

Kotlin makes it easy to write code that looks like what I mean. I love its extension functions, null handling, and approach to modelling data. It gives me plenty of reasons to leave Rust alone.

But **Rust infected my brain** anyway.

Rust took about a year to convince me. During that year, I learnt some Go, Haskell, and Zig. I also wrote a lot of Kotlin. Eventually I told a friend I was _fully oxidized_.

I could write a perfectly reasonable implementation in one of my favourite languages and still think:

<!-- This quotation uses check to mean verify, not select a checkbox. -->
<!-- vale Google.WordList = NO -->

> _Damn. I wish the language could check this part too._

<!-- vale Google.WordList = YES -->

## The compiler's many questions

At first, I found Rust difficult to learn.

I wanted to do something simple. **Not so fast**, says the compiler. **Who owns this value? How long must this reference remain valid? What else might modify this?**

I understood why the guarantees might help. I still resented all that thinking for such a small amount of code.

So I went back to Kotlin, where I didn't have to answer these questions before.

Well, I _didn't_.

Unfortunately, I started to ask the same questions there:

- Can I still use this object after that operation?
- Does my read-only reference point to something another part of the program can change?
- Can I use this library type with my abstraction without an adapter?

I love Kotlin. I also want the compiler to answer these questions for me. Rust made that a reasonable expectation, which is rather inconsiderate of it.

## Typestate, but better

Suppose I model a blog post. **I can edit and publish drafts. I can only read published posts.**

Kotlin handles this nicely with a generic state parameter and extension functions:

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

I keep the constructor and mutable storage internal to the library module. Outside that module, I get this public API:

```kotlin
val post = draft("Rust is a mind virus")
post.edit("Even when you write Kotlin")

val published = post.publish()

published.content()          // Fine
published.edit("Actually…")  // Compile error
```

This is **typestate**. The type represents the state and determines which operations I can use. Kotlin checks extension receivers statically, so it rejects `edit` on `Post<Published>`. [Kotlin extensions documentation](https://kotlinlang.org/docs/extensions.html)

Rust lets me express the same idea with `impl` blocks:

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

Both versions prevent editing through the published type's API. I'm happy with that.

Then I try to use the _original draft_ in Kotlin:

```kotlin
val post = draft("Rust is a mind virus")
val published = post.publish()

post.edit("One more change")  // Still allowed
val another = post.publish() // Also allowed
```

I want publication to **end the draft's lifecycle**, but my types fail to express that rule.

Rust lets me express this rule:

```rust
fn publish(self) -> Post<Published>
```

Taking `self` by value **consumes the draft**:

```rust
let draft = Post::new("Rust is a mind virus".into());
let published = draft.publish();

draft.edit("One more change".into()); // Compile error: moved
```

I want this guarantee when I finish a builder, commit a transaction, or close a session. In Kotlin, I can track whether the old object is still usable at runtime.

**Rust enforces this rule at compile time.**

## Read-only feels insufficient

Kotlin distinguishes read-only collection interfaces from mutable ones:

```kotlin
val names = mutableListOf("Alice", "Bob")
val view: List<String> = names
```

I can't call `add` through `view`. I can still change the same collection through `names`:

```kotlin
names.add("Charlie")

println(view) // [Alice, Bob, Charlie]
```

The read-only interface restricts **what I can do through this reference**. Another reference can still change the collection. [Kotlin collections documentation](https://kotlinlang.org/docs/collections-overview.html)

I can still modify the collection through `MutableList` while another part of the code iterates over it. With Kotlin's `ArrayList` on the JVM, structural changes during iteration can cause a `ConcurrentModificationException`, just like in Java. Java's [fail-fast iterators](https://docs.oracle.com/en/java/javase/25/docs/api/java.base/java/util/ArrayList.html) detect these changes on a best-effort basis.

Rust rejects the conflicting mutation in this example:

```rust
let mut names = vec![
    String::from("Alice"),
    String::from("Bob"),
];

let view = &names;

names.push(String::from("Charlie")); // Compile error

println!("{view:?}");
```

I use `view` after the attempted mutation, so its shared borrow must remain valid across that operation. Rust rejects the conflicting mutable access.

If I finish using the `view` first, mutation becomes available again:

```rust
let view = &names;
println!("{view:?}");

names.push(String::from("Charlie")); // Fine
```

In Kotlin, I can take a snapshot, use an [immutable collection](https://github.com/Kotlin/kotlinx.collections.immutable), or coordinate access. I have options.

A snapshot copies the collection. I prefer Rust's option: borrow the existing collection and let the compiler enforce the access rules.

I don't trust human review to enforce these access rules consistently. I want the compiler to enforce them for me.

Once again, **Rust rejects conflicting access at compile time.**

## Extension functions make me want traits

Kotlin's extension functions allow me to add a convenient operation to an existing type:

```kotlin
fun String.summary(): String = take(80)

val text = "A rather long article..."
println(text.summary())
```

I don't need to change or own `String` to do this. This is convenient, as you might imagine.

But then I want **an abstraction that several types can implement**:

```kotlin
interface Summarize {
    fun summary(): String
}

fun renderPreview(value: Summarize): String =
    value.summary()
```

My extension function can't make `String` implement that interface:

```kotlin
renderPreview(text) // Compile error
```

So I'd have to write an adapter:

```kotlin
class TextSummary(
    private val text: String,
) : Summarize {
    override fun summary(): String = text.take(80)
}

renderPreview(TextSummary(text))
```

This is ugly and verbose. I consider verbosity a particularly strong criticism of Kotlin, given how much Java boilerplate the language removes.

Rust's trait system lets me **implement my own trait directly for the existing type**.

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

**I own the trait, so I can implement it for `String`, even though I don't own `String`**, thanks to Rust's [orphan rules](https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules).

## Carcinization

I still love Kotlin, but Rust made me want more from a typed, compiled language.

Rust slowly wormed itself into my brain. It slowly coloured my every interaction with other programming languages.

"Can the compiler verify this?" I ask. "Why not?" I ask. "Can a library enforce this at compile time?" I ask.

And slowly, yet surely, I accepted Rust. Eventually, it was all I could think about. It was all I wanted to write.

And that's how I became _fully oxidized_.

---

**P.S. I haven't even talked about performance.**

Rust's performance gives me room to delay optimization and **focus on shipping features**. I spend less time thinking about performance and more time building the product. I find that rather funny for a language famous for low-level programming.

<!-- vale Google.FirstPerson = YES -->
<!-- vale Google.We = YES -->
