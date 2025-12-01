package io.lowkeylab.mindreadr.game

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

class GameTest {
    // Helper function to create a player
    private fun createPlayer(name: String): Player = Player(PlayerName(name))

    private val player1 = createPlayer("player1")
    private val player2 = createPlayer("player2")
    private val player3 = createPlayer("player3")
    private val player4 = createPlayer("player4")

    // Helper function to create a game
    private fun createGame(playerLimit: UInt = 2u): Game =
        Game(
            id = GameId("test-game"),
            playerLimit = playerLimit,
        )

    // Test Player Addition Scenarios

    @Test
    fun `addPlayer should add first player and keep game in WAITING_FOR_PLAYERS state`() {
        val game = createGame()
        game.addPlayer(player1)

        // Game should still be waiting for players (need 2 players)
        // We can't directly access state, but we can verify behavior
        // Adding another player should work
        game.addPlayer(player2)
    }

    @Test
    fun `addPlayer should transition to IN_PROGRESS when reaching player limit`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Game should now be IN_PROGRESS
        assertTrue(game.isInProgress())
    }

    @Test
    fun `addPlayer should count duplicate players toward limit (current behavior)`() {
        val game = createGame(playerLimit = 3u)
        game.addPlayer(player1)
        game.addPlayer(player1) // Duplicate instance counts toward limit
        // At this point players.size == 2
        game.addPlayer(player2) // Reaches limit of 3 (with duplicate)
        assertTrue(game.isInProgress())
        // Additional unique player cannot be added
        assertFailsWith<IllegalStateException> { game.addPlayer(player3) }
    }

    @Test
    fun `addPlayer should fail when game is not in WAITING_FOR_PLAYERS state`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Game is now IN_PROGRESS
        assertFailsWith<IllegalStateException> {
            game.addPlayer(player3)
        }
    }

    @Test
    fun `addPlayer should work with different player limits`() {
        val game = createGame(playerLimit = 4u)
        game.addPlayer(player1)
        game.addPlayer(player2)
        game.addPlayer(player3)

        // Should still be waiting for one more player
        game.addPlayer(player4)

        // Now should be IN_PROGRESS
        assertTrue(game.isInProgress())
    }

    // Test Guess Submission Logic

    @Test
    fun `addGuess should accept valid guesses from players`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Game is now IN_PROGRESS, should accept guesses
        game.addGuess(player1, "apple")
        game.addGuess(player2, "banana")

        // No exception means success
    }

    @Test
    fun `addGuess should prevent duplicate guesses from same player`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        game.addGuess(player1, "apple")
        game.addGuess(player1, "banana") // Should be ignored

        // If we try to complete the round, player2's guess should complete it
        game.addGuess(player2, "apple")

        // Round should complete since both players have guesses
    }

    @Test
    fun `addGuess should complete round when all players submit`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        game.addGuess(player1, "apple")
        game.addGuess(player2, "banana")

        // Round should be complete, and a new round should start
        // We can verify by adding more guesses
        game.addGuess(player1, "cherry")
        game.addGuess(player2, "date")
    }

    @Test
    fun `addGuess should progress to next round on non-unique guesses`() {
        val game = createGame(playerLimit = 3u)
        game.addPlayer(player1)
        game.addPlayer(player2)
        game.addPlayer(player3)

        // Round 1: Different guesses
        game.addGuess(player1, "apple")
        game.addGuess(player2, "banana")
        game.addGuess(player3, "cherry")

        // Should create round 2
        game.addGuess(player1, "dog")
        game.addGuess(player2, "elephant")
        game.addGuess(player3, "fox")

        // Should create round 3
    }

    @Test
    fun `addGuess should complete game when all guesses match`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        game.addGuess(player1, "apple")
        game.addGuess(player2, "apple") // Same guess

        // Game should be COMPLETED - all guesses matched
        assertTrue(game.hasEnded())
    }

    @Test
    fun `addGuess should complete game when all three players match`() {
        val game = createGame(playerLimit = 3u)
        game.addPlayer(player1)
        game.addPlayer(player2)
        game.addPlayer(player3)

        game.addGuess(player1, "winner")
        game.addGuess(player2, "winner")
        game.addGuess(player3, "winner")

        // Game should be COMPLETED - all guesses matched
        assertTrue(game.hasEnded())
    }

    @Test
    fun `addGuess should fail when no current round exists`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        // Only one player added, game still in WAITING_FOR_PLAYERS

        assertFailsWith<IllegalStateException> {
            game.addGuess(player1, "apple")
        }
    }

    // Test Player Removal Scenarios

    @Test
    fun `removePlayer should remove player during WAITING_FOR_PLAYERS`() {
        val game = createGame(playerLimit = 3u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        game.removePlayer(player1)

        // Should still be in WAITING_FOR_PLAYERS
        // Adding player3 and player4 should work
        game.addPlayer(player3)
        game.addPlayer(player4)

        // Now should be IN_PROGRESS
        assertTrue(game.isInProgress())
    }

    @Test
    fun `removePlayer should complete game when removed during IN_PROGRESS`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Game is IN_PROGRESS
        game.removePlayer(player1)

        // Game should have ended
        assertTrue(game.hasEnded())
    }

    @Test
    fun `removePlayer should handle non-existent player gracefully`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)

        // Removing a player that was never added
        game.removePlayer(player2)

        // Should still be in WAITING_FOR_PLAYERS
        game.addPlayer(player2)
    }

    @Test
    fun `removePlayer should fail when game is COMPLETED`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Complete the game by having all players guess the same thing
        game.addGuess(player1, "match")
        game.addGuess(player2, "match")

        // Game is now COMPLETED, removing player should fail
        assertFailsWith<IllegalStateException> {
            game.removePlayer(player1)
        }
    }

    // Test Round Behavior

    @Test
    fun `Round should correctly identify unique guesses`() {
        val round = Round(number = 1u)
        round.guesses[player1] = "apple"
        round.guesses[player2] = "banana"
        round.guesses[player3] = "apple" // Duplicate

        assertEquals(2, round.uniqueGuesses.size)
        assertTrue(round.uniqueGuesses.contains("apple"))
        assertTrue(round.uniqueGuesses.contains("banana"))
    }

    @Test
    fun `Round should identify all same guesses as one unique guess`() {
        val round = Round(number = 1u)
        round.guesses[player1] = "apple"
        round.guesses[player2] = "apple"
        round.guesses[player3] = "apple"

        assertEquals(1, round.uniqueGuesses.size)
        assertTrue(round.uniqueGuesses.contains("apple"))
    }

    @Test
    fun `Round should track round number correctly`() {
        val round1 = Round(number = 1u)
        assertEquals(1u, round1.number)

        val round2 = Round(number = 5u)
        assertEquals(5u, round2.number)
    }

    @Test
    fun `Round should map guesses correctly to player IDs`() {
        val round = Round(number = 1u)
        round.guesses[player1] = "guess1"
        round.guesses[player2] = "guess2"

        assertEquals("guess1", round.guesses[player1])
        assertEquals("guess2", round.guesses[player2])
        assertNull(round.guesses[player3])
    }

    // Test Edge Cases

    @Test
    fun `Game should handle single player limit edge case`() {
        val game = createGame(playerLimit = 1u)
        game.addPlayer(player1)

        // Should immediately transition to IN_PROGRESS
        assertTrue(game.isInProgress())
    }

    @Test
    fun `Game should handle multiple rounds correctly`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Round 1
        game.addGuess(player1, "round1-p1")
        game.addGuess(player2, "round1-p2")

        // Round 2
        game.addGuess(player1, "round2-p1")
        game.addGuess(player2, "round2-p2")

        // Round 3
        game.addGuess(player1, "round3-p1")
        game.addGuess(player2, "round3-p2")

        // Should still be in progress
        assertTrue(game.isInProgress())
    }

    @Test
    fun `Game should forbid reusing a guess from a previous round`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Round 1
        game.addGuess(player1, "alpha")
        game.addGuess(player2, "beta") // distinct -> new round created

        // Round 2 - attempting to reuse "alpha"
        assertFailsWith<IllegalStateException> { game.addGuess(player1, "alpha") }
        // Valid new guess still works
        game.addGuess(player1, "gamma")
        game.addGuess(player2, "delta")
    }

    @Test
    fun `Game should store guesses in lowercase and reject case-insensitive reuse`() {
        val game = Game(GameId("case-test"), playerLimit = 2u)
        val p1 = Player(PlayerName("Alice"))
        val p2 = Player(PlayerName("Bob"))
        game.addPlayer(p1)
        game.addPlayer(p2)
        // Round 1
        game.addGuess(p1, "Apple")
        game.addGuess(p2, "Banana")
        // Verify stored lowercase
        val round1 = game.getRounds().first()
        assertEquals("apple", round1.guesses[p1])
        assertEquals("banana", round1.guesses[p2])
        // Round 2 started (distinct guesses)
        // Attempt to reuse with different casing
        assertFailsWith<IllegalStateException> { game.addGuess(p1, "APPLE") }
        // New unique (case-insensitive) guess accepted
        game.addGuess(p1, "Cherry")
        game.addGuess(p2, "Date")
        val round2 = game.getRounds()[1]
        assertEquals("cherry", round2.guesses[p1])
        assertEquals("date", round2.guesses[p2])
    }

    @Test
    fun `addGuess should reject empty string`() {
        val game = Game(GameId("empty-guess"), playerLimit = 2u)
        val p1 = Player(PlayerName("A"))
        val p2 = Player(PlayerName("B"))
        game.addPlayer(p1)
        game.addPlayer(p2)
        assertFailsWith<IllegalStateException> { game.addGuess(p1, "") }
        // Valid guess after failure
        game.addGuess(p1, "value")
        game.addGuess(p2, "other")
        assertEquals(
            2,
            game
                .getRounds()
                .first()
                .guesses.size,
        )
    }

    @Test
    fun `Game should store accented characters lowercase and enforce reuse case-insensitively`() {
        val game = Game(GameId("accent-test"), playerLimit = 2u)
        val p1 = Player(PlayerName("José"))
        val p2 = Player(PlayerName("Zoë"))
        game.addPlayer(p1)
        game.addPlayer(p2)
        game.addGuess(p1, "ÁRBOL") // Spanish for tree with accent
        game.addGuess(p2, "café")
        val round1 = game.getRounds().first()
        assertEquals("árbol", round1.guesses[p1])
        assertEquals("café", round1.guesses[p2])
        // New round started because distinct
        assertFailsWith<IllegalStateException> { game.addGuess(p1, "ÁRBOL") } // reuse different casing
        game.addGuess(p1, "niño")
        game.addGuess(p2, "jalapeño")
        val round2 = game.getRounds()[1]
        assertEquals("niño", round2.guesses[p1])
        assertEquals("jalapeño", round2.guesses[p2])
    }

    @Test
    fun `addGuess should reject whitespace-only string`() {
        val game = Game(GameId("whitespace-guess"), playerLimit = 2u)
        val p1 = Player(PlayerName("A"))
        val p2 = Player(PlayerName("B"))
        game.addPlayer(p1)
        game.addPlayer(p2)
        assertFailsWith<IllegalStateException> { game.addGuess(p1, "   \t \n ") }
        game.addGuess(p1, "  spaced  ")
        game.addGuess(p2, "trimMe")
        val round = game.getRounds().first()
        assertEquals("spaced", round.guesses[p1])
        assertEquals("trimme", round.guesses[p2])
    }

    @Test
    fun `Game should terminate when round limit is reached`() {
        val game = Game(GameId("limit-test"), playerLimit = 2u, roundLimit = 3u)
        val p1 = Player(PlayerName("Alice"))
        val p2 = Player(PlayerName("Bob"))
        game.addPlayer(p1)
        game.addPlayer(p2)

        // Round 1: Different guesses
        game.addGuess(p1, "apple")
        game.addGuess(p2, "banana")
        assertTrue(game.isInProgress())

        // Round 2: Different guesses
        game.addGuess(p1, "cherry")
        game.addGuess(p2, "date")
        assertTrue(game.isInProgress())

        // Round 3: Different guesses - should reach round limit and terminate
        game.addGuess(p1, "elephant")
        game.addGuess(p2, "fox")

        // Game should be terminated after reaching round limit
        assertTrue(game.hasEnded())
        assertEquals(GameState.TERMINATED, game.getState())
        assertEquals(3, game.getRounds().size)
    }

    @Test
    fun `Game should complete normally if players match before round limit`() {
        val game = Game(GameId("complete-before-limit"), playerLimit = 2u, roundLimit = 10u)
        val p1 = Player(PlayerName("Alice"))
        val p2 = Player(PlayerName("Bob"))
        game.addPlayer(p1)
        game.addPlayer(p2)

        // Round 1: Different guesses
        game.addGuess(p1, "apple")
        game.addGuess(p2, "banana")

        // Round 2: Same guess - should complete
        game.addGuess(p1, "match")
        game.addGuess(p2, "match")

        // Game should be completed, not terminated
        assertTrue(game.hasEnded())
        assertEquals(GameState.COMPLETED, game.getState())
        assertEquals(2, game.getRounds().size)
        assertEquals("match", game.getFinalGuess())
    }

    @Test
    fun `Game should use default round limit of 10`() {
        val game = Game(GameId("default-limit"), playerLimit = 2u)
        assertEquals(10u, game.getRoundLimit())
    }

    @Test
    fun `Game should allow custom round limit`() {
        val game = Game(GameId("custom-limit"), playerLimit = 2u, roundLimit = 5u)
        assertEquals(5u, game.getRoundLimit())
    }
}
