use std::arch::x86_64::__cpuid_count;
fn main() {
    let v = __cpuid_count(0, 0);
    let vendor: Vec<u8> = [v.ebx, v.edx, v.ecx].iter().flat_map(|r| r.to_le_bytes()).collect();
    println!("vendor {} max_leaf {:#x}", String::from_utf8_lossy(&vendor), v.eax);
    let l7 = __cpuid_count(7, 0);
    println!("leaf7.ebx {:#010x}: PQM (QoS monitoring, bit 12) = {}, PQE (QoS enforcement / allocation, bit 15) = {}",
        l7.ebx, (l7.ebx >> 12) & 1, (l7.ebx >> 15) & 1);
    if v.eax >= 0xF {
        let f = __cpuid_count(0xF, 0);
        println!("leaf 0xF.0: max RMID {}, edx {:#x} (bit 1 = L3 monitoring)", f.ebx, f.edx);
    }
    if v.eax >= 0x10 {
        let e = __cpuid_count(0x10, 0);
        println!("leaf 0x10.0: ebx {:#x} (bit 1 = L3 allocation, bit 3 = memory bandwidth allocation)", e.ebx);
        if (e.ebx >> 1) & 1 == 1 {
            let l3 = __cpuid_count(0x10, 1);
            println!("L3 allocation: mask length {}, max class of service {}", (l3.eax & 0x1f) + 1, (l3.edx & 0xffff) + 1);
        }
    }
    let ext = __cpuid_count(0x8000_0000, 0);
    println!("max extended leaf {:#x}", ext.eax);
    if ext.eax >= 0x8000_0020 {
        let q = __cpuid_count(0x8000_0020, 0);
        println!("leaf 0x80000020.0 ebx {:#x} (AMD PQoS extended: bit 1 L3 bandwidth enforcement, bit 2 slow-memory)", q.ebx);
    }
}
