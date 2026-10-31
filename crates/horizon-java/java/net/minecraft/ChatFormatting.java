package net.minecraft;

/** Chat colours and styles (accepted by components; the Java layer's text is plain). */
public enum ChatFormatting {
    BLACK('0'), DARK_BLUE('1'), DARK_GREEN('2'), DARK_AQUA('3'), DARK_RED('4'), DARK_PURPLE('5'), GOLD('6'), GRAY('7'),
    DARK_GRAY('8'), BLUE('9'), GREEN('a'), AQUA('b'), RED('c'), LIGHT_PURPLE('d'), YELLOW('e'), WHITE('f'),
    OBFUSCATED('k'), BOLD('l'), STRIKETHROUGH('m'), UNDERLINE('n'), ITALIC('o'), RESET('r');

    private final char code;

    ChatFormatting(char code) {
        this.code = code;
    }

    public char getChar() {
        return code;
    }

    public boolean isFormat() {
        return code >= 'k' && code <= 'o';
    }

    public boolean isColor() {
        return !isFormat() && this != RESET;
    }

    public String getName() {
        return name().toLowerCase(java.util.Locale.ROOT);
    }

    @Override
    public String toString() {
        return "§" + code;
    }
}
