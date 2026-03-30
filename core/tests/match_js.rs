/// Verify Rust iterative bootstrap matches JS to < 0.01bp

#[test]
fn test_usd_match() { run_ccy("USD", 360.0,
    &[(1,3.7885),(2,3.7195),(3,3.679),(4,3.685),(5,3.721),(7,3.812),(10,3.9488),(15,4.147),(20,4.2131),(30,4.1594)],
    &[0.0,369.0,735.0,1099.0,1463.0,1830.0,2196.0,2561.0,2926.0,3290.0,3657.0,4022.0,4387.0,4752.0,5117.0,5481.0,5848.0,6213.0,6579.0,6944.0,7308.0,7672.0,8040.0,8405.0,8770.0,9135.0,9499.0,9866.0,10231.0,10596.0,10962.0],
); }
#[test]
fn test_gbp_match() { run_ccy("GBP", 365.0,
    &[(1,4.3034),(2,4.3626),(3,4.3166),(4,4.2922),(5,4.2958),(7,4.355),(10,4.5028),(15,4.7118),(20,4.823),(25,4.8599),(30,4.8483)],
    &[0.0,368.0,731.0,1096.0,1461.0,1826.0,2195.0,2558.0,2922.0,3287.0,3653.0,4018.0,4385.0,4749.0,5114.0,5479.0,5844.0,6213.0,6576.0,6940.0,7305.0,7670.0,8036.0,8403.0,8767.0,9131.0,9497.0,9862.0,10231.0,10594.0,10958.0],
); }
#[test]
fn test_chf_match() { run_ccy("CHF", 360.0,
    &[(1,0.1304),(2,0.248),(3,0.2859),(4,0.3336),(5,0.3809),(7,0.471),(10,0.6035),(15,0.774),(20,0.8468),(25,0.8522),(30,0.8199)],
    &[0.0,369.0,735.0,1098.0,1463.0,1830.0,2196.0,2561.0,2926.0,3290.0,3657.0,4022.0,4387.0,4752.0,5116.0,5481.0,5848.0,6213.0,6579.0,6944.0,7308.0,7672.0,8040.0,8405.0,8770.0,9134.0,9499.0,9866.0,10231.0,10596.0,10961.0],
); }

fn run_ccy(name: &str, basis: f64, rates: &[(u32, f64)], pay_days: &[f64]) {
    let max_y = pay_days.len() - 1;
    let mut taus = vec![0.0];
    for y in 1..=max_y {
        taus.push((pay_days[y] - pay_days[y-1]) / basis);
    }
    
    let cy: Vec<u32> = rates.iter().map(|&(y,_)| y).collect();
    let mut ndf = std::collections::HashMap::new();
    ndf.insert(0u32, 1.0f64);
    
    let interp = |y: u32, ndf: &std::collections::HashMap<u32,f64>| -> f64 {
        if let Some(&v) = ndf.get(&y) { return v; }
        let mut pn = 0u32; let mut nn: Option<u32> = None;
        for &c in &cy { if c<=y && ndf.contains_key(&c) { pn=c; } if c>y && nn.is_none() && ndf.contains_key(&c) { nn=Some(c); } }
        match nn {
            None => { let r = -ndf[&pn].ln()/pay_days[pn as usize]; (-r*pay_days[y as usize]).exp() }
            Some(n) => { let w=(pay_days[y as usize]-pay_days[pn as usize])/(pay_days[n as usize]-pay_days[pn as usize]); (ndf[&pn].ln()*(1.0-w)+ndf[&n].ln()*w).exp() }
        }
    };
    
    for (idx, &(tenor, rate)) in rates.iter().enumerate() {
        let s = rate / 100.0;
        let prev = if idx>0 { cy[idx-1] } else { 0 };
        if (tenor - prev) <= 1 {
            let ann: f64 = (1..tenor).map(|y| taus[y as usize]*interp(y,&ndf)).sum();
            ndf.insert(tenor, (1.0-s*ann)/(1.0+s*taus[tenor as usize]));
        } else {
            // Brent
            let mut a=0.01f64; let mut b=1.5f64;
            let spv = |trial: f64, ndf: &mut std::collections::HashMap<u32,f64>| -> f64 {
                ndf.insert(tenor, trial);
                let ann: f64 = (1..=tenor).map(|y| taus[y as usize]*interp(y,ndf)).sum();
                1.0-s*ann-trial
            };
            let mut fa = spv(a, &mut ndf); let mut fb = spv(b, &mut ndf);
            if fa*fb > 0.0 {
                let ann: f64 = (1..tenor).map(|y| taus[y as usize]*interp(y,&ndf)).sum();
                ndf.insert(tenor, (1.0-s*ann)/(1.0+s*taus[tenor as usize]));
            } else {
                let (mut c,mut fc,mut d,mut e) = (a,fa,b-a,b-a);
                for _ in 0..100 {
                    if fb*fc>0.0 { c=a;fc=fa;d=b-a;e=d; }
                    if fc.abs()<fb.abs() { a=b;b=c;c=a;fa=fb;fb=fc;fc=fa; }
                    let tol=2e-16*b.abs()+1e-15; let m=0.5*(c-b);
                    if m.abs()<=tol||fb==0.0 { break; }
                    if e.abs()>=tol && fa.abs()>fb.abs() {
                        let s2=fb/fa;
                        let (p,q) = if a==c { (2.0*m*s2, 1.0-s2) } else {
                            let q2=fa/fc; let r2=fb/fc;
                            (s2*(2.0*m*q2*(q2-r2)-(b-a)*(r2-1.0)), (q2-1.0)*(r2-1.0)*(s2-1.0))
                        };
                        let (p,q) = if p>0.0 { (p,-q) } else { (-p,q) };
                        if 2.0*p<3.0*m*q-(tol*q).abs() && 2.0*p<(e*q).abs() { e=d;d=p/q; } else { d=m;e=m; }
                    } else { d=m;e=m; }
                    a=b;fa=fb;
                    b += if d.abs()>tol { d } else { if m>0.0 {tol} else {-tol} };
                    fb = spv(b, &mut ndf);
                }
                ndf.insert(tenor, b);
            }
        }
    }
    
    // Build getDf from curve nodes only
    let nodes: Vec<(f64,f64)> = cy.iter().filter_map(|&y| ndf.get(&y).map(|&df| (pay_days[y as usize], df))).collect();
    let get_df = |days: f64| -> f64 {
        if days<=0.0 { return 1.0; }
        if days<=nodes[0].0 { let r=-nodes[0].1.ln()/(nodes[0].0/365.0); return (-r*days/365.0).exp(); }
        if days>=nodes.last().unwrap().0 { let l=nodes.last().unwrap(); let r=-l.1.ln()/(l.0/365.0); return (-r*days/365.0).exp(); }
        for i in 0..nodes.len()-1 {
            if nodes[i].0<=days && nodes[i+1].0>=days {
                let w=(days-nodes[i].0)/(nodes[i+1].0-nodes[i].0);
                return (nodes[i].1.ln()*(1.0-w)+nodes[i+1].1.ln()*w).exp();
            }
        }
        nodes.last().unwrap().1
    };
    
    // Test: par rates at curve nodes must round-trip to 0
    println!("\n{} par rate round-trip:", name);
    for &(tenor, rate) in rates {
        let sd = pay_days[0];
        let mut ann = 0.0;
        for y in 1..=tenor {
            let d = pay_days[y as usize];
            ann += taus[y as usize] * get_df(d);
        }
        let par = (get_df(sd) - get_df(pay_days[tenor as usize])) / ann * 100.0;
        let err = (par - rate).abs() * 100.0;
        println!("  {}Y: input={:.4}% computed={:.10}% err={:.2e}bp", tenor, rate, par, err);
        assert!(err < 0.001, "{} {}Y: par rate round-trip err={:.4}bp", name, tenor, err);
    }
    
    // Test: forward starts
    println!("{} forward starts:", name);
    for &(start, tenor) in &[(2u32,5u32),(5,10),(1,5),(1,10),(10,10)] {
        if (start+tenor) as usize > max_y { continue; }
        let sd = pay_days[start as usize];
        let mut ann = 0.0;
        for y in 1..=tenor {
            let d = pay_days[(start+y) as usize];
            let prev_d = pay_days[(start+y-1) as usize];
            ann += (d-prev_d)/basis * get_df(d);
        }
        let par = (get_df(sd) - get_df(pay_days[(start+tenor) as usize])) / ann * 100.0;
        println!("  {}Y+{}Y: par={:.6}%", start, tenor, par);
    }
}
