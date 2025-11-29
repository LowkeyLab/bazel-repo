package io.lowkeylab.mindreadr.app

import io.ktor.client.call.body
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.get
import io.ktor.serialization.kotlinx.json.json
import io.ktor.server.testing.testApplication
import io.lowkeylab.mindreadr.GamesSummary
import io.lowkeylab.mindreadr.configureGames
import io.lowkeylab.mindreadr.configureSerialization
import io.lowkeylab.mindreadr.configureSockets
import io.lowkeylab.mindreadr.game.GameService
import io.lowkeylab.mindreadr.game.InMemoryGameRepository
import io.lowkeylab.mindreadr.game.Player
import io.lowkeylab.mindreadr.game.PlayerFactory
import io.lowkeylab.mindreadr.game.PlayerName
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals

class GamesSummaryTest {
    // Simple test player factory to produce deterministic names
    private class TestPlayerFactory : PlayerFactory {
        private var idx = 0

        override fun create(): Player {
            idx += 1
            return Player(PlayerName("player$idx"))
        }

        override fun removeName(name: PlayerName) { /* no-op */ }
    }

    @Test
    fun `summaries reflect waiting, in-progress, and completed games`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSerialization()
                configureSockets()
                configureGames(gameService)
            }

            // Create games covering all states:
            // 1) Waiting: a newly created game with 0 players
            val waitingGame = gameService.createGame()
            assertEquals(true, gameService.getGameById(waitingGame.id)!!.isWaitingForPlayers())

            // 2) In-progress: create game and add two players
            val inProgressGame = gameService.createGame()
            gameService.addPlayerToGame(inProgressGame.id)
            gameService.addPlayerToGame(inProgressGame.id)
            // Ensure game has transitioned to IN_PROGRESS by adding two players
            assertEquals(true, gameService.getGameById(inProgressGame.id)!!.isInProgress())

            // 3) Completed: create game, add two players, have them submit identical guesses
            val completedGame = gameService.createGame()
            val c1 = gameService.addPlayerToGame(completedGame.id)
            val c2 = gameService.addPlayerToGame(completedGame.id)
            gameService.addGuessToGame(c1, completedGame.id, "apple")
            gameService.addGuessToGame(c2, completedGame.id, "apple")
            // Verify completed state
            assertEquals(true, gameService.getGameById(completedGame.id)!!.hasEnded())

            // Configure client deserialization for JSON
            val client =
                createClient {
                    install(ContentNegotiation) { json(Json) }
                }

            val responseSummary = client.get("/games/summary").body<GamesSummary>()

            assertEquals(1, responseSummary.waitingForPlayerGames)
            assertEquals(1, responseSummary.inProgressGames)
            assertEquals(1, responseSummary.completedGames)
        }
}
