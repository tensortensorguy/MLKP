# Generator for the SHA-512 IV and round constants in tools/cep_lint/src/sha512.rs.
# The constants are the first 64 bits of the fractional parts of the square roots of the
# first 8 primes (IV) and cube roots of the first 80 primes (K), computed with exact
# integer arithmetic so the derivation is reproducible without floating point.
def icbrt(n):
    if n == 0:
        return 0
    x = 1 << ((n.bit_length() + 2) // 3)
    while True:
        y = (2 * x + n // (x * x)) // 3
        if y >= x:
            break
        x = y
    while x * x * x > n:
        x -= 1
    while (x + 1) ** 3 <= n:
        x += 1
    return x

def isqrt(n):
    x = 1 << ((n.bit_length() + 1) // 2)
    while True:
        y = (x + n // x) // 2
        if y >= x:
            break
        x = y
    return x

def primes(count):
    ps, n = [], 2
    while len(ps) < count:
        if all(n % p for p in ps if p * p <= n):
            ps.append(n)
        n += 1
    return ps

K = [icbrt(p * (1 << 192)) & ((1 << 64) - 1) for p in primes(80)]
H = [isqrt(p * (1 << 128)) & ((1 << 64) - 1) for p in primes(8)]
print("H =", H)
print("K =", K)
