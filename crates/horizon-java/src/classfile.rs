//! The changes the Java layer makes to a mod's mixin class as it loads (never to anything else).
//!
//! A mixin class is usually `abstract`: Mixin never instantiates it, it merges it into its target.
//! The Java layer calls its handlers on instances of it instead, so:
//!
//! - [`concrete`]: `ACC_ABSTRACT` cleared, for a mixin whose target the Java API layer doesn't have.
//!   Its `@Shadow` methods stay abstract (`AbstractMethodError` if called).
//! - [`rebase`]: for a target the API layer has (`net/minecraft/server/MinecraftServer`...), the
//!   class's superclass becomes that API class, so an instance *is* the target (`this` casts to it)
//!   and stands for one target object. Its constructors go (they call the old superclass's, and
//!   the instance is made without one), and so do its abstract methods: a `@Shadow` method then
//!   resolves to the API class's method of the same name and descriptor.

/// Where the class's access flags are: right after the constant pool.
pub fn access_flags_at(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..4)? != [0xCA, 0xFE, 0xBA, 0xBE] {
        return None;
    }
    let count = u2(bytes, 8)?;
    let (mut at, mut i) = (10usize, 1usize);
    while i < count {
        let size = match *bytes.get(at)? {
            1 => 3 + u2(bytes, at + 1)?,
            3 | 4 | 9..=12 | 17 | 18 => 5,
            5 | 6 => {
                i += 1;
                9
            }
            7 | 8 | 16 | 19 | 20 => 3,
            15 => 4,
            _ => return None,
        };
        at += size;
        i += 1;
    }
    (at + 2 <= bytes.len()).then_some(at)
}

/// `bytes` with `ACC_ABSTRACT` cleared on the class (interfaces are left as they are).
pub fn concrete(bytes: &[u8]) -> Option<Vec<u8>> {
    let at = access_flags_at(bytes)?;
    let flags = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
    if flags & 0x0200 != 0 {
        return None;
    }
    let mut out = bytes.to_vec();
    out[at..at + 2].copy_from_slice(&(flags & !0x0400).to_be_bytes());
    Some(out)
}

/// A class file's parts the rewriting needs: the constant pool's UTF-8 entries and class
/// entries, and where each section starts.
struct Parsed<'a> {
    bytes: &'a [u8],
    /// Entry index to its offset (0 for unusable slots).
    entries: Vec<usize>,
    /// Offset of the access flags (right after the pool).
    access: usize,
}

fn u2(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]) as usize)
}

fn u4(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]) as usize)
}

impl<'a> Parsed<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        if bytes.get(..4)? != [0xCA, 0xFE, 0xBA, 0xBE] {
            return None;
        }
        let count = u2(bytes, 8)?;
        let mut entries = vec![0; count];
        let (mut at, mut i) = (10usize, 1usize);
        while i < count {
            entries[i] = at;
            let size = match *bytes.get(at)? {
                1 => 3 + u2(bytes, at + 1)?,
                3 | 4 | 9..=12 | 17 | 18 => 5,
                5 | 6 => {
                    i += 1;
                    9
                }
                7 | 8 | 16 | 19 | 20 => 3,
                15 => 4,
                _ => return None,
            };
            at += size;
            i += 1;
        }
        (at + 8 <= bytes.len()).then_some(Self {
            bytes,
            entries,
            access: at,
        })
    }

    fn utf8(&self, index: usize) -> Option<String> {
        let at = *self.entries.get(index)?;
        if at == 0 || self.bytes[at] != 1 {
            return None;
        }
        let len = u2(self.bytes, at + 1)?;
        Some(String::from_utf8_lossy(self.bytes.get(at + 3..at + 3 + len)?).into_owned())
    }

    fn class(&self, index: usize) -> Option<String> {
        let at = *self.entries.get(index)?;
        if at == 0 || self.bytes[at] != 7 {
            return None;
        }
        self.utf8(u2(self.bytes, at + 1)?)
    }

    /// The end of a field or method table starting at `at`, with each member's offsets.
    fn members(&self, at: usize) -> Option<(usize, Vec<(usize, usize)>)> {
        let count = u2(self.bytes, at)?;
        let mut out = Vec::with_capacity(count);
        let mut p = at + 2;
        for _ in 0..count {
            let start = p;
            let attrs = u2(self.bytes, p + 6)?;
            p += 8;
            for _ in 0..attrs {
                p += 6 + u4(self.bytes, p + 2)?;
            }
            if p > self.bytes.len() {
                return None;
            }
            out.push((start, p));
        }
        Some((p, out))
    }
}

/// A class's name, superclass and interfaces (internal names).
pub fn names(bytes: &[u8]) -> Option<(String, Option<String>, Vec<String>)> {
    let p = Parsed::new(bytes)?;
    let this = p.class(u2(bytes, p.access + 2)?)?;
    let sup = p.class(u2(bytes, p.access + 4)?);
    let n = u2(bytes, p.access + 6)?;
    let interfaces = (0..n)
        .map(|i| u2(bytes, p.access + 8 + 2 * i).and_then(|c| p.class(c)))
        .collect::<Option<Vec<_>>>()?;
    Some((this, sup, interfaces))
}

/// The class file's major version (65 is Java 21, 69 Java 25).
pub fn major_version(bytes: &[u8]) -> Option<u16> {
    (bytes.get(..4)? == [0xCA, 0xFE, 0xBA, 0xBE]).then(|| u16::from_be_bytes([bytes[6], bytes[7]]))
}

/// The Java version a class file needs (major 69 is Java 25).
pub fn java_version(bytes: &[u8]) -> Option<u32> {
    major_version(bytes).map(|m| u32::from(m).saturating_sub(44))
}

/// `bytes` as a subclass of `superclass` (internal name): concrete, its constructors and abstract
/// methods removed (module docs). None for an interface or a class file it can't read.
pub fn rebase(bytes: &[u8], superclass: &str) -> Option<Vec<u8>> {
    let p = Parsed::new(bytes)?;
    let flags = u16::from_be_bytes([bytes[p.access], bytes[p.access + 1]]);
    if flags & 0x0200 != 0 {
        return None;
    }
    let count = u2(bytes, 8)?;
    let n_interfaces = u2(bytes, p.access + 6)?;
    let fields_at = p.access + 8 + 2 * n_interfaces;
    let (methods_at, _) = p.members(fields_at)?;
    let (attributes_at, methods) = p.members(methods_at)?;
    let kept: Vec<(usize, usize)> = methods
        .into_iter()
        .filter(|&(start, _)| {
            let access = u2(bytes, start).unwrap_or(0);
            let name = u2(bytes, start + 2).and_then(|i| p.utf8(i));
            access & 0x0400 == 0 && name.as_deref() != Some("<init>")
        })
        .collect();
    // The new superclass's name and class entries go at the pool's end: no index moves.
    let name = superclass.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + name.len() + 6);
    out.extend_from_slice(&bytes[..8]);
    out.extend_from_slice(&u16::try_from(count + 2).ok()?.to_be_bytes());
    out.extend_from_slice(&bytes[10..p.access]);
    out.push(1);
    out.extend_from_slice(&u16::try_from(name.len()).ok()?.to_be_bytes());
    out.extend_from_slice(name);
    out.push(7);
    out.extend_from_slice(&u16::try_from(count).ok()?.to_be_bytes());
    out.extend_from_slice(&(flags & !0x0400).to_be_bytes());
    out.extend_from_slice(&bytes[p.access + 2..p.access + 4]);
    out.extend_from_slice(&u16::try_from(count + 1).ok()?.to_be_bytes());
    out.extend_from_slice(&bytes[p.access + 6..methods_at]);
    out.extend_from_slice(&u16::try_from(kept.len()).ok()?.to_be_bytes());
    for (start, end) in kept {
        out.extend_from_slice(&bytes[start..end]);
    }
    out.extend_from_slice(&bytes[attributes_at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A class with a UTF-8, a class, a long (two slots) and a method handle in its pool.
    fn class(flags: u16) -> Vec<u8> {
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65, 0, 6];
        b.extend([1, 0, 1, b'A']); // #1 utf8 "A"
        b.extend([7, 0, 1]); // #2 class
        b.extend([5, 0, 0, 0, 0, 0, 0, 0, 7]); // #3-4 long
        b.extend([15, 1, 0, 2]); // #5 method handle
        b.extend(flags.to_be_bytes());
        b.extend([0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn clears_abstract_on_classes_only() {
        let abstract_class = class(0x0421);
        assert_eq!(access_flags_at(&abstract_class), Some(30));
        let out = concrete(&abstract_class).unwrap();
        assert_eq!(&out[30..32], &0x0021u16.to_be_bytes());
        assert_eq!(out.len(), abstract_class.len());
        assert_eq!(concrete(&class(0x0601)), None, "an interface stays one");
        assert_eq!(concrete(&class(0x0021)).unwrap(), class(0x0021));
        assert_eq!(access_flags_at(b"nope"), None);
        assert_eq!(access_flags_at(&abstract_class[..29]), None);
    }

    /// A class `M` (super `java/lang/Object`, abstract) with a constructor, an abstract method,
    /// a plain method and a class attribute.
    fn mixin() -> Vec<u8> {
        let utf8 = |s: &str| {
            let mut v = vec![1];
            v.extend((s.len() as u16).to_be_bytes());
            v.extend(s.as_bytes());
            v
        };
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 69, 0, 11];
        for s in [
            "M",
            "java/lang/Object",
            "<init>",
            "()V",
            "shadow",
            "handler",
            "Code",
            "SourceFile",
        ] {
            b.extend(utf8(s)); // #1..#8
        }
        b.extend([7, 0, 1]); // #9 class M
        b.extend([7, 0, 2]); // #10 class Object
        b.extend(0x0421u16.to_be_bytes());
        b.extend([0, 9, 0, 10, 0, 0, 0, 0]); // this, super, no interfaces, no fields
        b.extend([0, 3]);
        b.extend([0, 1, 0, 3, 0, 4, 0, 1, 0, 7, 0, 0, 0, 2, 0xAB, 0xCD]); // <init> with Code
        b.extend([0x04, 0x01, 0, 5, 0, 4, 0, 0]); // abstract shadow
        b.extend([0, 1, 0, 6, 0, 4, 0, 1, 0, 7, 0, 0, 0, 1, 0xB1]); // handler with Code
        b.extend([0, 1, 0, 8, 0, 0, 0, 2, 0, 1]); // SourceFile
        b
    }

    #[test]
    fn rebases_a_mixin_onto_its_target() {
        let m = mixin();
        assert_eq!(
            names(&m),
            Some(("M".into(), Some("java/lang/Object".into()), vec![]))
        );
        assert_eq!(java_version(&m), Some(25));
        let out = rebase(&m, "net/minecraft/server/MinecraftServer").unwrap();
        assert_eq!(
            names(&out),
            Some((
                "M".into(),
                Some("net/minecraft/server/MinecraftServer".into()),
                vec![]
            ))
        );
        let p = Parsed::new(&out).unwrap();
        let flags = u2(&out, p.access).unwrap();
        assert_eq!(flags & 0x0400, 0, "concrete");
        let n_interfaces = u2(&out, p.access + 6).unwrap();
        let (methods_at, _) = p.members(p.access + 8 + 2 * n_interfaces).unwrap();
        let (end, methods) = p.members(methods_at).unwrap();
        let names: Vec<String> = methods
            .iter()
            .map(|(s, _)| p.utf8(u2(&out, s + 2).unwrap()).unwrap())
            .collect();
        assert_eq!(names, ["handler"], "the constructor and the shadow go");
        assert_eq!(
            &out[end..],
            &[0, 1, 0, 8, 0, 0, 0, 2, 0, 1],
            "attributes kept"
        );
        assert_eq!(rebase(&class(0x0601), "X"), None, "an interface");
        assert_eq!(rebase(b"nope", "X"), None);
    }
}
