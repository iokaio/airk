// SPDX-License-Identifier: Apache-2.0
namespace archive {
template <typename T> struct Box { T value; };
int count(int value) { return value + 1; }
}
@interface Archive
- (const char *)title;
@end
@implementation Archive
- (const char *)title { return "café 🦀"; }
@end
