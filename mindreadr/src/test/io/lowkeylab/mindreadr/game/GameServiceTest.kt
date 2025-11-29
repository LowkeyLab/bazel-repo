package io.lowkeylab.mindreadr.game

import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class GameServiceTest {
    // Deterministic player factory for tests
    private class TestPlayerFactory : PlayerFactory {
        private var idx = 0

        override fun create(): Player {
            idx += 1
            return Player(PlayerName("player$idx"))
        }

        override fun removeName(name: PlayerName) { /* no-op */ }
    }

    private fun newService(): GameService {
        val playerFactory = TestPlayerFactory()
        val gameRepository = InMemoryGameRepository()
        return GameService(playerFactory, gameRepository)
    }

    @Test
    fun `createGame should create distinct games and be waiting for players`() =
        runBlocking {
            val service = newService()
            val g1 = service.createGame()
            val g2 = service.createGame()
            assertTrue(g1.id != g2.id)
            assertTrue(g1.isWaitingForPlayers())
            assertTrue(g2.isWaitingForPlayers())
            assertEquals(2, service.getAllGames().size)
        }

    @Test
    fun `getGameById returns null for missing game and game for existing`() =
        runBlocking {
            val service = newService()
            val game = service.createGame()
            val existing = service.getGameById(game.id)
            val missing = service.getGameById(GameId("nope"))
            assertNotNull(existing)
            assertNull(missing)
        }

    @Test
    fun `addPlayerToGame transitions game to IN_PROGRESS at limit`() =
        runBlocking {
            val service = newService()
            val game = service.createGame()
            // Default player limit is 2
            val p1 = service.addPlayerToGame(game.id)
            assertTrue(service.getGameById(game.id)!!.isWaitingForPlayers())
            val p2 = service.addPlayerToGame(game.id)
            val updated = service.getGameById(game.id)!!
            assertTrue(updated.isInProgress())
            assertEquals(2, updated.getPlayerCount())
            // Ensure players got distinct names
            assertTrue(p1.name != p2.name)
        }

    @Test
    fun `addGuessToGame progresses rounds and can complete game`() =
        runBlocking {
            val service = newService()
            val game = service.createGame()
            val p1 = service.addPlayerToGame(game.id)
            val p2 = service.addPlayerToGame(game.id)
            // First guess -> still round 1
            service.addGuessToGame(p1, game.id, "apple")
            val afterFirstGuess = service.getGameById(game.id)!!
            assertTrue(afterFirstGuess.isInProgress())
            assertEquals(1, afterFirstGuess.getRoundCount())
            // Second guess differs -> new round created
            service.addGuessToGame(p2, game.id, "banana")
            val afterSecondGuess = service.getGameById(game.id)!!
            assertEquals(2, afterSecondGuess.getRoundCount())
            // Round 2 identical guesses -> complete
            service.addGuessToGame(p1, game.id, "match")
            service.addGuessToGame(p2, game.id, "match")
            val completed = service.getGameById(game.id)!!
            assertTrue(completed.hasEnded())
            assertEquals("match", completed.getFinalGuess())
        }

    @Test
    fun `removePlayerFromGame during IN_PROGRESS completes game`() =
        runBlocking {
            val service = newService()
            val game = service.createGame()
            val p1 = service.addPlayerToGame(game.id)
            service.addPlayerToGame(game.id)
            assertTrue(service.getGameById(game.id)!!.isInProgress())
            service.removePlayerFromGame(p1, game.id)
            val afterRemoval = service.getGameById(game.id)!!
            assertTrue(afterRemoval.hasEnded())
            // getFinalGuess is only defined for completion via matching guesses; do not assert here
        }

    @Test
    fun `getGameSummaries returns counts across states`() =
        runBlocking {
            val service = newService()
            // Waiting game
            service.createGame()
            // In-progress game
            val inProgress = service.createGame()
            service.addPlayerToGame(inProgress.id)
            service.addPlayerToGame(inProgress.id)
            // Completed game via matching guesses
            val completed = service.createGame()
            val c1 = service.addPlayerToGame(completed.id)
            val c2 = service.addPlayerToGame(completed.id)
            service.addGuessToGame(c1, completed.id, "same")
            service.addGuessToGame(c2, completed.id, "same")
            val summary = service.getGameSummaries()
            assertEquals(1, summary.waitingForPlayerGames)
            assertEquals(1, summary.inProgressGames)
            assertEquals(1, summary.completedGames)
        }
}
