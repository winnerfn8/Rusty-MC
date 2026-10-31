package net.minecraft.world;

/** What an interaction did: it took the click ({@link #consumesAction}) or let it through. */
public interface InteractionResult {
    Success SUCCESS = new Success("client");
    Success SUCCESS_SERVER = new Success("server");
    Success CONSUME = new Success("none");
    Fail FAIL = new Fail();
    Pass PASS = new Pass();
    TryEmptyHandInteraction TRY_WITH_EMPTY_HAND = new TryEmptyHandInteraction();

    default boolean consumesAction() {
        return false;
    }

    /** It took the click; which side swings the arm. */
    final class Success implements InteractionResult {
        private final String swing;

        private Success(String swing) {
            this.swing = swing;
        }

        @Override
        public boolean consumesAction() {
            return true;
        }

        @Override
        public String toString() {
            return "Success[" + swing + "]";
        }
    }

    /** It failed. */
    final class Fail implements InteractionResult {
        private Fail() {}
    }

    /** It let the click through. */
    final class Pass implements InteractionResult {
        private Pass() {}
    }

    /** It let the click through to the empty hand. */
    final class TryEmptyHandInteraction implements InteractionResult {
        private TryEmptyHandInteraction() {}
    }
}
