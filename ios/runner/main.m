#import <UIKit/UIKit.h>

extern void gpui_ios_run_demo(const char *name);

@interface GPUIAppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation GPUIAppDelegate

- (BOOL)application:(UIApplication *)app
    didFinishLaunchingWithOptions:(NSDictionary *)opts {
    // Demo name is passed as a process argument by ios/run.
    // Falls back to "hello_world" when launched manually from Xcode.
    NSArray *args = [[NSProcessInfo processInfo] arguments];
    const char *demo = (args.count > 1) ? [args[1] UTF8String] : "hello_world";
    gpui_ios_run_demo(demo);
    return YES;
}

@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil,
                                 NSStringFromClass([GPUIAppDelegate class]));
    }
}
