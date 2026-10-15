// SPDX-License-Identifier: Apache-2.0
package archive
import scala.collection.mutable.ListBuffer
trait Named { def render(value: String): String }
class Archive extends Named {
  val caption = "café 🦀"
  type Label = String
  def render(value: String): String = value + caption
}
object Main { def run(): Unit = println("archive") }
