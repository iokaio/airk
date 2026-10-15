// SPDX-License-Identifier: Apache-2.0
library archive;
import 'dart:math' as math;
part 'archive_part.dart';
mixin Named { String title = 'café 🦀'; }
enum State { ready, done }
class Archive with Named {
  void render() { print(title); }
}
extension Display on Archive {
  String display() => title;
}
