---
title: "Personal Landing Page"
description: "My personal website"
cover: "../../assets/website.png"
tags: ["Astro", "Tailwind", "Deno", "TypeScript"]
featured: true
links:
  github: "https://github.com/LowkeyLab/deno-monorepo/tree/main/website"
  demo: "https://www.tacascer.com"
---

## Features

- Responsive design that works on all devices
- Project showcase with filtering capabilities
- Fast performance with minimal JavaScript
- Built using modern web technologies

## Tech Stack

### Astro

[Astro](https://astro.build/)'s content-driven principle and focus on [island architecture](https://jasonformat.com/islands-architecture/) is the missing web framework I was looking for.

Before Astro, web frameworks delineated themselves into "sites with only HTML content" and "sites with only JavaScript". You either went for a static site generator like [Jekyll](https://jekyllrb.com/) and forgo all your interactivity, or you went with an SPA framework like [React](https://react.dev/) and forgo using content-oriented file formats like Markdown. You either had an inherently SEO-friendly website, or you had an inherently interactive one.

What about [Ruby on Rails](https://rubyonrails.org/), [Flask](https://flask.palletsprojects.com/en/stable/), and the like? These frameworks require you to render your HTML entirely on the server; interactivity was provided by shipping some (probably untested) JavaScript. You would use something like [jQuery](https://jquery.com/) to do DOM manipulation and Ajax. It's very hard to fully take over on the client side and reap some of its benefits, such as lower latency, easier context management, etc. The web moved to SPAs for a reason.

The most popular framework that offered both easy content management and client interactivity is [Gatsby](https://www.gatsbyjs.com/). But Gatsby wedded you to React, and it was still using React to render content exclusively on the client side with JavaScript.

In recent years, server-side rendering has been making a return (thanks to [Next.js](https://nextjs.org/)). As a response, React added [server components](https://react.dev/reference/rsc/server-components), which Gatsby uses to introduce [partial hydration](https://www.gatsbyjs.com/docs/conceptual/partial-hydration). While this is great for those who are already in the React ecosystem, I happen to not like React and its programming model very much. Still in the early days of my web development journey, I want to experiment with different approaches to web development, and Astro's flexible integration with all kinds of front-end frameworks is a much-needed feature for me.

For a more detailed take on the JavaScript framework landscape, visit Allan Pike's [blog on the same topic](https://allenpike.com/2025/javascript-fatigue-ssr?).

### TailwindCSS

[TailwindCSS](https://tailwindcss.com/) has become a mainstay in the web development circle for styling applications.

I strongly believe that the previous state-of-the-art of complicated CSS stylesheets with creatively named CSS classes to avoid collision is obsolete; it was an artifact of the web before React pioneered a "component-based" design approach. First, [styled-components](https://styled-components.com/) came along to really push the idea of coupling the CSS with HTML content inside a component. Then, Angular introduced scoping a `.css` file to a component as its approach to [component styling](https://angular.dev/guide/components/styling). More recently, bundlers like [Vite](https://vite.dev/) formalized the pattern of "scoping CSS to a component" with [CSS Modules](https://github.com/css-modules/css-modules). This pattern of CSS usage allows for easily debuggable CSS (because the "Cascading" part is gone), and eliminates the need for coming up with unique names for CSS classes.

People started packaging components with pre-defined styles to enforce a design system. There is [React Bootstrap](https://react-bootstrap.netlify.app/) for [Bootstrap](https://getbootstrap.com/), [MaterialUI](https://mui.com/material-ui/) for [Material Design](https://m3.material.io/), etc. Companies would distribute a component library packaged with well-defined CSS styles for each. The fundamental reusable block in web design is no longer **content** (HTML) and **style** (CSS), but **components** (HTML + CSS).

The problem with components is style customizability. Unless your components have a base [headless component](https://martinfowler.com/articles/headless-component.html), you can't just swap out the style of an element inside that component by passing in `textSize="large"`. Without those props, you need to write long and verbose CSS that would (hopefully) override the styles defined in the component. But how does the new font size relate to the rest of the site? Most of the time, when you change the style in a component, you're changing it in relation to another portion of your site (e.g., make this text larger than this other text). You'd need to know the internals of how the style of the other element is specified: Is the font size in `px`, `rem`, or `em`?

Good design systems have well-defined [CSS variables](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_cascading_variables/Using_CSS_custom_properties) for things like "small font size", "medium font size", etc. to be overridden. Great design systems have well-defined classes for these common styles, like `text-sm`, `text-md`, because changing the font size also means changing the line height. TailwindCSS is that great design system.

### DaisyUI

[DaisyUI](https://daisyui.com/) builds upon the primitives provided by TailwindCSS to provide more concise CSS classes for common parts of a website (buttons, navbars, etc.).

You can think of it like Bootstrap but designed on top of TailwindCSS.

I wanted a pure CSS library that provided some good-looking out-of-the-box classes. It had to be a pure CSS library because different parts of this website could be using different front-end libraries to render (a feature of Astro). A pure CSS library allows me to have a unified style in all parts of the website, regardless of the rendering technology.

I also wanted a library that was built upon TailwindCSS. [Tailwind Typography](https://github.com/tailwindlabs/tailwindcss-typography) allows styling of external vanilla HTML contents (like Markdown files) with Tailwind primitives. Having a CSS library that was built upon TailwindCSS allows content-driven parts of my website (like this page you're reading) to look good without having to specify any custom stylings.

### Deno

[Deno](https://deno.com/) is a server-side JavaScript runtime (yes, there are more than one) by the creator of [Node.js](https://nodejs.org/en).

In any project, I try to introduce a technology that is "white hot" in terms of risk. Deno is that technology for this project. I use Deno as the package manager and web server. All configurations and package management are done in Deno, making the most of Deno 2.0's [interop with Node.js](https://docs.deno.com/runtime/fundamentals/node/).

There are some growing pains. The biggest one is that documentation for all other technologies in this website (Astro, Vite, etc.) doesn't demonstrate how to get it working with Deno. Of course, thanks to Deno's interop with Node.js, things actually work out of the box (which was a pleasant surprise).

One of the biggest confusions I had was how to deploy my site. I'm utilizing Deno's [workspace](https://docs.deno.com/runtime/fundamentals/workspaces/) support, which has a section on [containerizing a workspace member](https://docs.deno.com/runtime/fundamentals/workspaces/#important-note-for-containerization). The documentation shows that to containerize a workspace member, you only need to include the root `deno.json`, the workspace member of interest, and all workspace members it depends on. However, this is not true, as `deno install` needs every workspace member to be present. I've [submitted the issue](https://github.com/denoland/deno/issues/28365) to the Deno team.

Another annoyance is the need to still include a `tsconfig.json` file, even though Deno provides TypeScript out of the box. This is because Astro (and many other JS libraries) don't generate types with valid [triple slash directives](https://docs.deno.com/runtime/reference/ts_config_migration/#triple-slash-directive), so a `tsconfig` is still needed to make TypeScript language servers happy.
