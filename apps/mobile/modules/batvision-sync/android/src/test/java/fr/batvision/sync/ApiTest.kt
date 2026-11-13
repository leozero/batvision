package fr.batvision.sync

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test

class ApiTest {
  @Test
  fun `a connection lost while reading is the server out of reach`() {
    val cause = IOException("unexpected end of stream")
    val e = assertThrows(Unreachable::class.java) { reading<Int> { throw cause } }
    assertSame(cause, e.cause)
    assertEquals(3, reading { 3 })
  }

  @Test
  fun `other errors while reading stay what they are`() {
    assertThrows(IllegalStateException::class.java) { reading<Int> { error("not the network") } }
  }
}
