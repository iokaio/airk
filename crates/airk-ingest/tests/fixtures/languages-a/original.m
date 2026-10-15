// SPDX-License-Identifier: Apache-2.0
#import <Foundation/Foundation.h>
@protocol Named
- (NSString *)title;
@end
@interface Archive : NSObject <Named>
@property(nonatomic, copy) NSString *caption;
- (void)render:(NSString *)value with:(int)count;
@end
@interface Archive (Display)
- (NSString *)display;
@end
@implementation Archive
- (void)render:(NSString *)value with:(int)count { NSLog(@"café 🦀"); }
- (NSString *)title { [self render:@"archive" with:1]; return self.caption; }
@end
