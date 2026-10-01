import java.util.List;

/** A greeter. */
public class Greeter {
    @Override
    public String toString() {
        return "Hello, " + List.of(1, 2).size();
    }
}
