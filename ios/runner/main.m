#import <UIKit/UIKit.h>

extern void gpui_ios_run_demo(const char *name);

@interface GPUIAppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation GPUIAppDelegate

- (BOOL)application:(UIApplication *)app
    didFinishLaunchingWithOptions:(NSDictionary *)opts {
    NSProcessInfo *processInfo = [NSProcessInfo processInfo];
    NSArray<NSString *> *args = [processInfo arguments];
    NSDictionary<NSString *, NSString *> *env = [processInfo environment];
    NSUserDefaults *defaults = [NSUserDefaults standardUserDefaults];
    NSString *const kLastDemoKey = @"GPUI_IOS_LAST_DEMO";

    // Primary launch channel from ios/run: explicit environment variable.
    NSString *demo = env[@"GPUI_IOS_DEMO"];

    // Fallback: look for a known demo name in process args (passed by launchers).
    if (demo.length == 0) {
        NSSet<NSString *> *knownDemos = [NSSet setWithArray:@[
            @"hello_world",
            @"touch",
            @"text",
            @"lifecycle",
            @"combined",
            @"scroll",
            @"text_input",
            @"vertical_scroll",
            @"horizontal_scroll",
            @"pinch",
            @"rotation",
            @"controls",
            @"native_controls",
            @"safe_area",
            @"layout_showcase",
            @"keyboard_ime",
            @"file_picker",
            @"clipboard",
            @"file_drop",
        ]];

        for (NSInteger i = args.count - 1; i >= 1; i--) {
            NSString *candidate = args[i];
            if ([knownDemos containsObject:candidate]) {
                demo = candidate;
                break;
            }
        }
    }

    // If this launch had no explicit demo (for example, icon reopen),
    // continue with the most recently selected demo.
    if (demo.length == 0) {
        NSString *savedDemo = [defaults stringForKey:kLastDemoKey];
        if (savedDemo.length > 0) {
            demo = savedDemo;
        }
    }

    if (demo.length == 0) {
        demo = @"hello_world";
    }

    [defaults setObject:demo forKey:kLastDemoKey];

    NSLog(@"[GPUI-iOS] launch args=%@ selected_demo=%@", args, demo);
    gpui_ios_run_demo([demo UTF8String]);
    return YES;
}

@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil,
                                 NSStringFromClass([GPUIAppDelegate class]));
    }
}
