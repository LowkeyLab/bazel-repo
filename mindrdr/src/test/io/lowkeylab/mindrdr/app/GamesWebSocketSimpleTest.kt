package io.lowkeylab.mindrdr.app

import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.plugins.websocket.webSocket
import io.ktor.server.testing.testApplication
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import io.lowkeylab.mindrdr.OutgoingMessage
import io.lowkeylab.mindrdr.configureGames
import io.lowkeylab.mindrdr.configureSerialization
import io.lowkeylab.mindrdr.configureSockets
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.InMemoryGameRepository
import io.lowkeylab.mindrdr.game.Player
import io.lowkeylab.mindrdr.game.PlayerFactory
import io.lowkeylab.mindrdr.game.PlayerName
import kotlinx.serialization.json.Json
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class GamesWebSocketSimpleTest {
    private class TestPlayerFactory : PlayerFactory {
        private val counter = AtomicInteger(0)

        override fun create(): Player {
            val count = counter.incrementAndGet()
            return Player(PlayerName("player$count"))
        }

        override fun removeName(name: PlayerName) {}
    }

    @Test
    fun `single client connection test`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient { install(WebSockets) }

            client.webSocket("/games/${game.id.id}/live") {
                // Receive first 2 messages (PlayerJoined and GameState)
                val frame1 = incoming.receive()
                assertIs<Frame.Text>(frame1)
                val msg1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                assertIs<OutgoingMessage.PlayerJoined>(msg1)
                assertEquals("player1", msg1.player.name.name)

                val frame2 = incoming.receive()
                assertIs<Frame.Text>(frame2)
                val msg2 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                assertIs<OutgoingMessage.GameState>(msg2)
                assertEquals(1, msg2.game.getPlayerCount())
            }
        }
}
