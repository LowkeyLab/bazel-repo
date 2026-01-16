---
title: "Guess The Word"
description: "A real-time multiplayer word guessing game"
tags: ["Node.js", "WebSocket", "Svelte", "Tailwind", "Postgres"]
startDate: 2025-01-19
endDate: 2025-02-07
featured: false
links:
  github: "https://github.com/LowkeyLab/guess-the-word"
---

## Overview

Guess The Word is a real-time multiplayer word guessing game where players take turns guessing a word until they come to an agreement.

## Features

- Real-time multiplayer gameplay using WebSockets
- Responsive design that works on desktop and mobile devices
- Player authentication using Supabase

## Tech Stack

### SvelteKit & Svelte

[Svelte](https://svelte.dev/) is a new rendering library that was created at [The Guardian](https://www.theguardian.com/us) and was ranked as the most admired web framework in [Stack Overflow's 2024 Developer Survey](https://survey.stackoverflow.co/2024/technology#admired-and-desired).

[SvelteKit](https://svelte.dev/docs/kit/introduction) is the server-side part of Svelte that enables Server Side Rendering, REST APIs, etc.

Svelte is part of a new breed of web frameworks that compile component source code down into JavaScript, instead of being a pure JavaScript library. Svelte's popularity is why React is introducing the [React Compiler](https://react.dev/learn/react-compiler).

Being a web framework in the 2020s, Svelte is also keenly aware of the need for Server Side Rendering and its interplay with Client Side Rendering. SvelteKit very explicitly models where [data loading is happening](https://svelte.dev/docs/kit/load): server-only, client-only, or both. This explicit modeling of the data loading lifecycle introduces some interesting problems.

I had a lot of trouble understanding how to [perform authentication](https://svelte.dev/docs/kit/auth), because knowing where to store the JWT token and when to trust it were mental models I didn't have before. In the end, I went with [server hooks](https://svelte.dev/docs/kit/hooks#Server-hooks), storing the auth details in [`locals`](https://svelte.dev/docs/kit/hooks#Server-hooks-locals), then performing server-side loading of a user's credentials every time they navigate to a page that needs authentication. I couldn't trust the JWT token in the `Authorization` header handed to me from the client-side, because the client could have modified it. The [Lucia guide](https://lucia-auth.com/sessions/cookies/sveltekit) was incredibly helpful in understanding how to set up authentication.

### WebSockets

[Socket.io](https://socket.io/) is the WebSockets implementation of choice for this project.

Originally I wanted to use [Spring's STOMP support](https://docs.spring.io/spring-framework/reference/web/websocket/stomp.html) with a Kotlin backend. Unfortunately, the [STOMP.js](https://github.com/stomp-js/stompjs) library isn't really well-known in the JavaScript ecosystem; this is probably a reflection of how STOMP is mostly a messaging middleware integration, and that usually happens in the backend. I also got tired of having to duplicate the message object models on both Kotlin and JavaScript sides, and other constraints in this project compelled me to look for a pure JavaScript solution. You can still find the original Kotlin backend [here](https://github.com/LowkeyLab/gradle-monorepo/tree/main/guess-the-word).

### Node.js & Express.js

An [Express.js](https://expressjs.com/) server in the backend provides:

- RESTful API endpoints for game creation and deletion
- Socket.io server to manage game state
- User authentication and session management

The server talks to [Supabase](https://supabase.com/), an open-source [Firebase](https://firebase.google.com/) alternative. Supabase allowed me to use standard [Postgres](https://www.postgresql.org/) instead of proprietary constructs only available in Firebase.

Games are created by making an entry in the `Games` table, and only events from authenticated users can trigger the server to create it. After a game ends, the entry is deleted.

Originally, the plan was to store finished games and then run some kind of analytics on them. I quickly realized that not enough people would be playing the game, so any kind of analytics generated would be overkill. Maybe one day I'll get around to it.

## Notes

This project has since been rewritten as [mindreadr](/projects/mindreadr).
