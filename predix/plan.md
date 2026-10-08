# 🏟️ Predix: the private circle arena

**Predix** is a social prediction platform designed for friend groups, roommates, and coworkers. Instead of betting against the world, you wager "Clout" against the people you know best on the things that happen in your daily life.

---

## 👥 Core concept and circles

The app is organized into **Circles**—private, invite-only spaces. Everything (Clout balances, contests, and leaderboards) is scoped to the specific group.

### 1. Circle management

- **Private Circles:** create a Circle (e.g., "The Sunday Football Crew") and generate a unique invite code.
- **Join via Code:** friends join by entering the code, instantly seeing the Circle's open Contests.
- **Members + Clout Ledger:** each Member in a Circle has a Clout balance (starts at 1000 on join) tracked per-circle, not globally.
- **Creators:** the Circle Creator is recorded (CreatorID) and remains the Circle owner for administrator actions.

### 1.1 Spontaneous contests

- **Spontaneous Contests:** users can initiate quick, informal contests within their Circles, allowing for immediate engagement on current events or challenges.
- **Quick Setup:** A simple interface for creating spontaneous contests that require minimal setup, encouraging more frequent participation.
- **Instant Notifications:** members receive real-time notifications for new spontaneous contests, ensuring they can join in the fun without delay.
- **Clout Stakes:** members can wager varying amounts of Clout on these spontaneous contests, fostering a dynamic contest environment.
- **Resolution Process:** similar to regular contests, spontaneous contests can be resolved by the creator or through community voting, adding a layer of social interaction.

### 2. Contests and predictions

- **Contests:** A Contest is a question with multiple Options, created by a user and scoped to one or more Circles. Domain status: OPEN → RESOLVED (CLOSED is reserved for future pause states).
- **Options:** contest options are enumerated choices (Option IDs) that members can stake on.
- **Predictions:** A Prediction records a member's stake of Clout on an Option at a timestamp. Clout must be positive; invalid options are rejected.
- **Resolution:** the Contest Creator resolves a Contest by marking the winning Option; the result is stored as ResultOptionID.
- **Lifecycle:** contests capture created/expiry timestamps so clients can close prediction windows when expired.

### 3. Social Interaction

- **The "Trash Talk" Ticker:** A live comment feed for each Contest where friends can post gifs, images, or messages as the event unfolds.
- **Activity Heatmap:** see which friends are the most active "instigators" (creating Contests) vs. "sharks" (winning the most Clout).
- **Direct Challenges:** send a 1v1 Contest invite (Snap-Bet) to a specific friend that only they can see and accept.

### 4. Group rankings (the "local" leaderboard)

- **The Big Kahuna:** the current top earner in the Circle.
- **The Underdog:** the person with the highest win rate but lowest total wagers.
- **Hall of Shame:** A list of the most spectacularly failed "All-In" bets in the group's history.

---

## 📈 Social hooks for growth

<!-- Preserve the quoted example notification text. -->
<!-- vale Google.Exclamation = NO -->

- **Shareable Recap Cards:** at the end of the week, generate a "Circle Recap" image showing the biggest winner and the funniest failed prediction to share in group chats.
- **Nudge Notifications:** "Hey! 3 people just bet against you on the 'Dish Washing' poll. Want to raise the stakes?"

<!-- vale Google.Exclamation = YES -->
