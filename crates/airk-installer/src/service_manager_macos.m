// SPDX-License-Identifier: Apache-2.0
// Bundle-scoped OS operations only. No label-based launchctl mutation or old-job adoption.
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#include <stdint.h>
#include <errno.h>
#include <stdlib.h>

// Historical sealed definitions keep their decoder; new registrations launch
// the selected main as the service itself, without a foreground child wrapper.
static NSString *loginEntry(NSString *version) {
    if (![version isKindOfClass:[NSString class]] || version.length > 64) return nil;
    NSArray<NSString *> *parts = [version componentsSeparatedByString:@"."];
    if (parts.count != 3) return nil;
    unsigned long long values[3];
    for (NSUInteger index = 0; index < 3; index++) {
        NSString *part = parts[index];
        if (part.length == 0 || (part.length > 1 && [part hasPrefix:@"0"])) return nil;
        const char *bytes = part.UTF8String;
        for (NSUInteger digit = 0; digit < part.length; digit++) {
            if (bytes[digit] < '0' || bytes[digit] > '9') return nil;
        }
        errno = 0;
        char *end = NULL;
        values[index] = strtoull(bytes, &end, 10);
        if (errno != 0 || end == bytes || *end != '\0') return nil;
    }
    BOOL direct = values[0] > 1 ||
        (values[0] == 1 && (values[1] > 8 || (values[1] == 8 && values[2] >= 14)));
    return direct ? @"login-serve" : @"login-foreground";
}

int airk_macos_login(int action, const char *plist_name, const char *expected_bundle,
                     int *observed, int64_t *error_code) {
    @autoreleasepool {
        *observed = -1;
        *error_code = 0;
        if (@available(macOS 13.0, *)) {
            NSString *plist = [NSString stringWithUTF8String:plist_name];
            NSString *expected = [NSString stringWithUTF8String:expected_bundle];
            NSBundle *bundle = [NSBundle mainBundle];
            NSString *version = bundle.infoDictionary[@"CFBundleVersion"];
            NSString *entry = loginEntry(version);
            if (expected == nil || plist == nil ||
                entry == nil || ![version isEqual:bundle.infoDictionary[@"CFBundleShortVersionString"]] ||
                ![bundle.bundleIdentifier isEqualToString:@"io.iokaio.airk.service"] ||
                ![bundle.infoDictionary[@"CFBundleExecutable"] isEqualToString:@"airk"] ||
                ![bundle.bundleURL.path.stringByResolvingSymlinksInPath isEqualToString:expected] ||
                !([plist isEqualToString:@"io.iokaio.airk.service.stable.plist"] ||
                  [plist isEqualToString:@"io.iokaio.airk.service.preview.plist"])) {
                return -2;
            }
            NSString *channel = [plist isEqualToString:@"io.iokaio.airk.service.stable.plist"] ? @"stable" : @"preview";
            NSURL *definitionURL = [bundle.bundleURL URLByAppendingPathComponent:
                [@"Contents/Library/LaunchAgents/" stringByAppendingString:plist]];
            NSDictionary *attributes = [[NSFileManager defaultManager] attributesOfItemAtPath:definitionURL.path error:NULL];
            NSNumber *definitionSize = attributes[NSFileSize];
            if (definitionSize == nil || definitionSize.unsignedLongLongValue > 16384) {
                return -2;
            }
            NSData *definitionData = [NSData dataWithContentsOfURL:definitionURL];
            if (definitionData == nil || definitionData.length > 16384) {
                return -2;
            }
            id definition = [NSPropertyListSerialization propertyListWithData:definitionData
                options:NSPropertyListImmutable format:NULL error:NULL];
            NSDictionary *expectedDefinition = @{
                @"Label": [@"io.iokaio.airk.service." stringByAppendingString:channel],
                @"BundleProgram": @"Contents/MacOS/airk",
                @"ProgramArguments": @[@"airk", @"runtime", entry, @"--channel", channel],
                @"RunAtLoad": @YES, @"KeepAlive": @{@"SuccessfulExit": @NO},
                @"ThrottleInterval": @30, @"ProcessType": @"Background"
            };
            if (![definition isKindOfClass:[NSDictionary class]] ||
                ![(NSDictionary *)definition isEqualToDictionary:expectedDefinition]) {
                return -2;
            }
            SMAppService *service = [SMAppService agentServiceWithPlistName:plist];
            NSError *error = nil;
            BOOL success = YES;
            if (action == 1) {
                if (service.status == SMAppServiceStatusRequiresApproval) {
                    return -5;
                }
                success = [service registerAndReturnError:&error];
            } else if (action == 2) {
                success = [service unregisterAndReturnError:&error];
            } else if (action != 0) {
                return -2;
            }
            if (!success) {
                *error_code = (int64_t)error.code;
                return -3;
            }
            switch (service.status) {
                case SMAppServiceStatusNotRegistered: *observed = 0; break;
                case SMAppServiceStatusEnabled: *observed = 1; break;
                case SMAppServiceStatusRequiresApproval: *observed = 2; break;
                case SMAppServiceStatusNotFound: *observed = 3; break;
                default: return -4;
            }
            return 0;
        }
        return -1;
    }
}
