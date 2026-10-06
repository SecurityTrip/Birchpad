#import <Foundation/Foundation.h>

// A greeting, twice.
@interface Greeting : NSObject
@property (nonatomic, copy) NSString *name;
- (NSString *)render;
@end

@implementation Greeting
- (NSString *)render {
    return [NSString stringWithFormat:@"Hello, %@! Hello, %@!", self.name, self.name];
}
@end

int main(void) {
    Greeting *greeting = [Greeting new];
    greeting.name = @"world";
    NSLog(@"%@", [greeting render]);
    return 0;
}

@import Foundation;

static int retry(int times) {
    int tries = 0;
again:
    if (tries++ < times) goto again;
    return tries;
}
