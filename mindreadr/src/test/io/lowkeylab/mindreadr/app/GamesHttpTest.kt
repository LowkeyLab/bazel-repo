package io.lowkeylab.mindreadr.app

import io.ktor.client.call.body
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpStatusCode
import io.ktor.serialization.kotlinx.json.json
import io.ktor.server.testing.testApplication
import io.lowkeylab.mindreadr.GameDto
import io.lowkeylab.mindreadr.GamesSummary
import io.lowkeylab.mindreadr.configureGames
import io.lowkeylab.mindreadr.configureSerialization
import io.lowkeylab.mindreadr.game.GameService
import io.lowkeylab.mindreadr.game.GameState
import io.lowkeylab.mindreadr.game.InMemoryGameRepository
import io.lowkeylab.mindreadr.game.Player
import io.lowkeylab.mindreadr.game.PlayerFactory
import io.lowkeylab.mindreadr.game.PlayerName
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GamesHttpTest {
    private class TestPlayerFactory : PlayerFactory {
        private var counter = 0

        override fun create(): Player {
            counter += 1
            return Player(PlayerName("player$counter"))
        }

        override fun removeName(name: PlayerName) { // no-op
        }
    }

    @Test
    fun `post games creates game`() =
        testApplication {
            val service = GameService(TestPlayerFactory(), InMemoryGameRepository())
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }

            val response: HttpResponse = client.post("/games")
            assertEquals(HttpStatusCode.OK, response.status)
            val dto = response.body<GameDto>()
            assertEquals(2u, dto.playerLimit) // default
            assertEquals(GameState.WAITING_FOR_PLAYERS, dto.state)
        }

    @Test
    fun `get games id returns game`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val service = GameService(TestPlayerFactory(), repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            val game = service.createGame()
            val response = client.get("/games/${game.id.id}")
            assertEquals(HttpStatusCode.OK, response.status)
            val dto = response.body<GameDto>()
            assertEquals(game.id.id, dto.id.id)
        }

    @Test
    fun `get games id not found`() =
        testApplication {
            val service = GameService(TestPlayerFactory(), InMemoryGameRepository())
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            val response = client.get("/games/does-not-exist")
            assertEquals(HttpStatusCode.NotFound, response.status)
        }

    @Test
    fun `get games status waiting`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val service = GameService(TestPlayerFactory(), repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            // create two games waiting
            service.createGame()
            service.createGame()
            val response = client.get("/games?status=waiting_for_players")
            assertEquals(HttpStatusCode.OK, response.status)
            val list = response.body<List<GameDto>>()
            assertEquals(2, list.size)
            assertTrue(list.all { it.state == GameState.WAITING_FOR_PLAYERS })
        }

    @Test
    fun `get games status in progress`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val service = GameService(TestPlayerFactory(), repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            // Create a game and advance to IN_PROGRESS by adding players
            val g = service.createGame()
            service.addPlayerToGame(g.id)
            service.addPlayerToGame(g.id)
            val response = client.get("/games?status=in_progress")
            assertEquals(HttpStatusCode.OK, response.status)
            val list = response.body<List<GameDto>>()
            assertEquals(1, list.size)
            assertEquals(GameState.IN_PROGRESS, list.first().state)
        }

    @Test
    fun `get games status completed`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val service = GameService(TestPlayerFactory(), repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            val g = service.createGame()
            service.addPlayerToGame(g.id)
            service.addPlayerToGame(g.id)
            // Submit identical guesses to end game
            val players = g.getPlayers()
            g.addGuess(players[0], "apple")
            g.addGuess(players[1], "apple")
            val response = client.get("/games?status=completed")
            assertEquals(HttpStatusCode.OK, response.status)
            val list = response.body<List<GameDto>>()
            assertEquals(1, list.size)
            assertEquals(GameState.COMPLETED, list.first().state)
        }

    @Test
    fun `get games invalid status`() =
        testApplication {
            val service = GameService(TestPlayerFactory(), InMemoryGameRepository())
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            val response = client.get("/games?status=bogus")
            assertEquals(HttpStatusCode.BadRequest, response.status)
        }

    @Test
    fun `get games without status message`() =
        testApplication {
            val service = GameService(TestPlayerFactory(), InMemoryGameRepository())
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            val response = client.get("/games")
            assertEquals(HttpStatusCode.OK, response.status)
            val text = response.body<String>()
            assertTrue(text.contains("status"))
        }

    @Test
    fun `get games summary counts`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val factory = TestPlayerFactory()
            val service = GameService(factory, repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            // waiting: 2
            service.createGame()
            service.createGame()
            // in progress: 1
            val g2 = service.createGame()
            service.addPlayerToGame(g2.id)
            service.addPlayerToGame(g2.id)
            // completed: 1
            val g3 = service.createGame()
            service.addPlayerToGame(g3.id)
            service.addPlayerToGame(g3.id)
            val players = g3.getPlayers()
            g3.addGuess(players[0], "x")
            g3.addGuess(players[1], "x")
            val response = client.get("/games/summary")
            assertEquals(HttpStatusCode.OK, response.status)
            val summary = response.body<GamesSummary>()
            assertEquals(2, summary.waitingForPlayerGames)
            assertEquals(1, summary.inProgressGames)
            assertEquals(1, summary.completedGames)
        }

    @Test
    fun `get games case insensitive status`() =
        testApplication {
            val repo = InMemoryGameRepository()
            val service = GameService(TestPlayerFactory(), repo)
            application {
                configureSerialization()
                configureGames(service)
            }
            val client = createClient { install(ContentNegotiation) { json(Json) } }
            service.createGame()
            val response = client.get("/games?status=WaItInG_FoR_PlaYeRs")
            assertEquals(HttpStatusCode.OK, response.status)
            val list = response.body<List<GameDto>>()
            assertEquals(1, list.size)
        }
}
