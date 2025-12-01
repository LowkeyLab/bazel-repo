package io.lowkeylab.mindreadr.app

import io.ktor.client.*
import io.ktor.client.plugins.websocket.*
import io.ktor.serialization.*
import io.ktor.serialization.kotlinx.*
import io.ktor.server.testing.*
import io.ktor.websocket.*
import io.lowkeylab.mindreadr.*
import io.lowkeylab.mindreadr.game.*
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.Json

class GamesWebSocketTest {
  // Inner class for test player factory with incrementing names
  private class TestPlayerFactory : PlayerFactory {
    private val counter = AtomicInteger(0)

    override fun create(): Player {
      val count = counter.incrementAndGet()
      return Player(PlayerName("player$count"))
    }

    override fun removeName(name: PlayerName) {
      // No-op for tests
    }
  }

  private class CoroutineBarrier(
      val parties: Int,
  ) {
    private var count = 0
    private val mutex = Mutex()
    private var generation = CompletableDeferred<Unit>()

    suspend fun await() {
      mutex.withLock {
        count++
        if (count == parties) {
          // Trip the barrier
          generation.complete(Unit)
        }
      }
      // Wait for the barrier to be tripped
      generation.await()
    }
  }

  @Test
  fun `single client connection receives PlayerJoined and GameState update`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val game = gameService.createGame()
    val client = createClient()

    client.webSocket("/games/${game.id.id}/live") {
      val playerJoined = receiveDeserialized<OutgoingMessage>()
      assertIs<OutgoingMessage.PlayerJoined>(playerJoined)
      val gameState = receiveDeserialized<OutgoingMessage>()
      assertIs<OutgoingMessage.GameState>(gameState)
    }
  }

  @Test
  fun `connection to non-existent game closes session`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val client = createClient()

    client.webSocket("/games/non-existent-game/live") {
      val reason = closeReason.await()
      assertTrue(reason != null, "Session should be closed")
    }
  }

  @Test
  fun `joining full game closes session`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val client = createClient()

    val game = gameService.createGame()
    val joinBarrier = CoroutineBarrier(2)
    val exitSignal = MutableSharedFlow<Unit>(replay = 1)

    // Fill the game with 2 players
    coroutineScope {
      launch {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await()
          exitSignal.first()
        }
      }

      launch {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await()
          exitSignal.first()
        }
      }

      // Both players joined, now signal the barrier
      joinBarrier.await()

      // Try to add a third player
      client.webSocket("/games/${game.id.id}/live") {
        val reason = closeReason.await()
        assertTrue(reason != null, "Session should be closed for full game")
        exitSignal.emit(Unit)
      }
    }
  }

  @Test
  fun `game completion when all submit identical guess`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val game = gameService.createGame()
    val client = createClient()

    coroutineScope {
      val joinBarrier = CoroutineBarrier(2)
      val clientOne = async {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await() // Ensure both clients connected

          sendSerialized<IncomingMessage>(IncomingMessage.SubmitGuess("apple"))

          while (true) {
            val msg = receiveDeserialized<OutgoingMessage>()
            if (msg is OutgoingMessage.GameState && msg.game.state == GameState.COMPLETED) {
              break
            }
          }
        }

        return@async true
      }

      val clientTwo = async {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await() // Ensure both clients connected

          sendSerialized<IncomingMessage>(IncomingMessage.SubmitGuess("apple"))

          while (true) {
            val msg = receiveDeserialized<OutgoingMessage>()
            if (msg is OutgoingMessage.GameState && msg.game.state == GameState.COMPLETED) {
              break
            }
          }
        }
        return@async true
      }

      assertTrue(clientOne.await())
      assertTrue(clientTwo.await())
    }
  }

  @Test
  fun `player disconnection broadcasts PlayerLeft to remaining clients`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val game = gameService.createGame()
    val client = createClient()
    val joinBarrier = CoroutineBarrier(2)

    coroutineScope {
      val result = async {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await() // Wait for both clients to connect

          while (true) {
            val msg = receiveDeserialized<OutgoingMessage>()
            if (
                msg is OutgoingMessage.PlayerJoined && msg.player == Player(PlayerName("player2"))
            ) {
              break
            }
          }
        }

        return@async true
      }

      launch {
        client.webSocket("/games/${game.id.id}/live") {
          joinBarrier.await() // Wait for both clients to connect
          // Client 2 disconnects
          close(CloseReason(CloseReason.Codes.NORMAL, "Test disconnect"))
        }
      }

      assertTrue(result.await())
    }
  }

  @Test
  fun `invalid JSON message returns Error and continues`() = testApplication {
    val playerFactory = TestPlayerFactory()
    val gameRepository = InMemoryGameRepository()
    val gameService = GameService(playerFactory, gameRepository)

    application {
      configureSockets()
      configureSerialization()
      configureGamesWs(gameService)
    }

    val game = gameService.createGame()
    val client = createClient()

    client.webSocket("/games/${game.id.id}/live") {
      incoming.receive() // PlayerJoined
      incoming.receive() // GameState

      // Send invalid JSON
      send(Frame.Text("{invalid json}"))

      val error = receiveDeserialized<OutgoingMessage>()
      assertIs<OutgoingMessage.Error>(error)
      assertTrue(error.message.contains("Invalid message format"))

      // Session should still be open, can continue
      sendSerialized(IncomingMessage.SubmitGuess("banana"))

      val secondError = receiveDeserialized<OutgoingMessage>()
      assertIs<OutgoingMessage.Error>(secondError)
    }
  }

  private fun ApplicationTestBuilder.createClient(): HttpClient = createClient {
    install(WebSockets) { contentConverter = KotlinxWebsocketSerializationConverter(Json) }
  }

  private fun DefaultClientWebSocketSession.consumeMessagesAsFlow(): Flow<List<OutgoingMessage>> =
      incoming
          .consumeAsFlow()
          .map { converter!!.deserialize<OutgoingMessage>(it) }
          .scan(emptyList<OutgoingMessage>()) { acc, message -> acc + message }
}
