package io.lowkeylab.guesstheword.game

import io.lowkeylab.guesstheword.player.PlayerId
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

class GameTest {
    private val player1 = PlayerId("player1")
    private val player2 = PlayerId("player2")
    private val player3 = PlayerId("player3")
    private val player4 = PlayerId("player4")

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
        // We can verify by checking that adding another player fails
        assertFailsWith<IllegalStateException> {
            game.addPlayer(player3)
        }
    }

    @Test
    fun `addPlayer should handle duplicate players by ignoring them`() {
        val game = createGame(playerLimit = 3u)
        game.addPlayer(player1)
        game.addPlayer(player1) // Duplicate
        game.addPlayer(player2)
        game.addPlayer(player3)

        // Should succeed - duplicate was ignored, so we have exactly 3 players
        // Game should be IN_PROGRESS now
        assertFailsWith<IllegalStateException> {
            game.addPlayer(player4)
        }
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
        assertFailsWith<IllegalStateException> {
            game.addPlayer(PlayerId("player5"))
        }
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
        // When completed, no new round is created, so adding guess should fail
        assertFailsWith<IllegalStateException> {
            game.addGuess(player1, "banana")
        }
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
        // Now with the updated implementation, adding guess should throw
        assertFailsWith<IllegalStateException> {
            game.addGuess(player1, "loser")
        }
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
        assertFailsWith<IllegalStateException> {
            game.addPlayer(PlayerId("player5"))
        }
    }

    @Test
    fun `removePlayer should complete game when removed during IN_PROGRESS`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        // Game is IN_PROGRESS
        game.removePlayer(player1)

        // Game should be COMPLETED
        // Verify by trying to add a player - should fail
        assertFailsWith<IllegalStateException> {
            game.addPlayer(player3)
        }

        // Also verify adding guess fails
        assertFailsWith<IllegalStateException> {
            game.addGuess(player2, "apple")
        }
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
        assertFailsWith<IllegalStateException> {
            game.addPlayer(player2)
        }
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
    }

    @Test
    fun `addPlayer should return game instance for chaining`() {
        val game = createGame(playerLimit = 2u)
        val result = game.addPlayer(player1)

        assertEquals(game, result)
    }

    @Test
    fun `addGuess should return game instance for chaining`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)
        game.addPlayer(player2)

        val result = game.addGuess(player1, "test")

        assertEquals(game, result)
    }

    @Test
    fun `removePlayer should return game instance for chaining`() {
        val game = createGame(playerLimit = 2u)
        game.addPlayer(player1)

        val result = game.removePlayer(player1)

        assertEquals(game, result)
    }

    @Test
    fun `GameState enum should have expected values`() {
        val states = GameState.entries
        assertEquals(3, states.size)
        assertTrue(states.contains(GameState.WAITING_FOR_PLAYERS))
        assertTrue(states.contains(GameState.IN_PROGRESS))
        assertTrue(states.contains(GameState.COMPLETED))
    }
}
