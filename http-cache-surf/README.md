<!DOCTYPE html>
<html lang="en" id="oranda" class="dark axo">
  <head>
    <title>http-cache</title>
    
    
      <link rel="icon" href="/favicon.ico" />
    
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    
      <meta name="description" content="An HTTP caching middleware" />
      <meta property="og:description" content="An HTTP caching middleware" />
    
    <meta property="og:type" content="website" />
    <meta property="og:title" content="http-cache" />
    
    
    
    <meta http-equiv="Permissions-Policy" content="interest-cohort=()" />
    <link rel="stylesheet" href="/oranda-v0.6.1.css" />
    
      <link rel="stylesheet" href="/custom.css" />
    
    
  </head>
  <body>
    <div class="container">
      <div class="page-body">
        
          <div class="repo_banner">
            <a href="https://github.com/06chaynes/http-cache">
              <div class="github-icon" aria-hidden="true"></div>
              Check out our GitHub!
            </a>
          </div>
        

        <main>
          <header>
            
            <h1 class="title">http-cache</h1>
            
  <nav class="nav">
    <ul>
      <li><a href="/">Home</a></li>

      
        
          <li><a href="/./http-cache/CHANGELOG/">http-cache changelog</a></li>
        
          <li><a href="/./http-cache-reqwest/README/">http-cache-reqwest</a></li>
        
          <li><a href="/./http-cache-reqwest/CHANGELOG/">http-cache-reqwest changelog</a></li>
        
          <li><a href="/./http-cache-surf/README/">http-cache-surf</a></li>
        
          <li><a href="/./http-cache-surf/CHANGELOG/">http-cache-surf changelog</a></li>
        
          <li><a href="/./http-cache-quickcache/README/">http-cache-quickcache</a></li>
        
          <li><a href="/./http-cache-quickcache/CHANGELOG/">http-cache-quickcache changelog</a></li>
        
      

      

      
        <li><a href="/book/">Docs</a></li>
      

      
        <li><a href="/funding/">Funding</a></li>
      

      
    </ul>
  </nav>

          </header>

          
  
    <h1>http-cache-surf</h1>
<p><a href="https://github.com/06chaynes/http-cache/actions/workflows/http-cache-surf.yml" rel="noopener noreferrer"><img src="https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-surf.yml?label=CI&amp;style=for-the-badge" alt="CI"></a>
<a href="https://crates.io/crates/http-cache-surf" rel="noopener noreferrer"><img src="https://img.shields.io/crates/v/http-cache-surf?style=for-the-badge" alt="Crates.io"></a>
<a href="https://docs.rs/http-cache-surf" rel="noopener noreferrer"><img src="https://img.shields.io/docsrs/http-cache-surf?style=for-the-badge" alt="Docs.rs"></a>
<a href="https://app.codecov.io/gh/06chaynes/http-cache" rel="noopener noreferrer"><img src="https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge" alt="Codecov"></a>
<img src="https://img.shields.io/crates/l/http-cache-surf?style=for-the-badge" alt="Crates.io"></p>
<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">
<p>A caching middleware that follows HTTP caching rules,
thanks to <a href="https://github.com/kornelski/rusty-http-cache-semantics" rel="noopener noreferrer">http-cache-semantics</a>.
By default, it uses <a href="https://github.com/zkat/cacache-rs" rel="noopener noreferrer">cacache</a> as the backend cache manager.
Should likely be registered after any middleware modifying the request.</p>
<h2>Minimum Supported Rust Version (MSRV)</h2>
<p>1.82.0</p>
<h2>Install</h2>
<p>With <a href="https://github.com/killercup/cargo-edit#Installation" rel="noopener noreferrer">cargo add</a> installed :</p>
<pre style="background-color:#263238;"><span style="color:#82aaff;">cargo add http-cache-surf
</span></pre>

<h2>Example</h2>
<pre style="background-color:#263238;"><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache_surf</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">Cache</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> CacheMode</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> CACacheManager</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> HttpCache</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> HttpCacheOptions</span><span style="color:#89ddff;">};
</span><span style="color:#eeffff;">
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">async_std::main</span><span style="color:#89ddff;">]
</span><span style="color:#eeffff;">async </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">main</span><span style="color:#89ddff;">() -&gt; </span><span style="color:#eeffff;">surf</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Result</span><span style="color:#89ddff;">&lt;()&gt; {
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> req </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">surf</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">get</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching</span><span style="color:#89ddff;">");
</span><span style="color:#eeffff;">    surf</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">client</span><span style="color:#89ddff;">()
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">with</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Cache</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">HttpCache </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">          mode</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">CacheMode</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Default</span><span style="color:#89ddff;">,
</span><span style="color:#eeffff;">          manager</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">CACacheManager</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">default</span><span style="color:#89ddff;">(),
</span><span style="color:#eeffff;">          options</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">HttpCacheOptions</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">default</span><span style="color:#89ddff;">(),
</span><span style="color:#eeffff;">        </span><span style="color:#89ddff;">}))
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">send</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">req</span><span style="color:#89ddff;">)
</span><span style="color:#eeffff;">        .await</span><span style="color:#89ddff;">?;
</span><span style="color:#eeffff;">    </span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(())
</span><span style="color:#89ddff;">}
</span></pre>

<h2>Features</h2>
<p>The following features are available. By default <code>manager-cacache</code> is enabled.</p>
<ul>
<li><code>manager-cacache</code> (default): enable <a href="https://github.com/zkat/cacache-rs" rel="noopener noreferrer">cacache</a>, a high-performance disk cache, backend manager.</li>
<li><code>manager-moka</code> (disabled): enable <a href="https://github.com/moka-rs/moka" rel="noopener noreferrer">moka</a>, a high-performance in-memory cache, backend manager.</li>
</ul>
<h2>Documentation</h2>
<ul>
<li><a href="https://docs.rs/http-cache-surf" rel="noopener noreferrer">API Docs</a></li>
</ul>
<h2>License</h2>
<p>Licensed under either of</p>
<ul>
<li>Apache License, Version 2.0
(<a href="https://github.com/06chaynes/http-cache/blob/main/LICENSE-APACHE" rel="noopener noreferrer">LICENSE-APACHE</a> or <a href="http://www.apache.org/licenses/LICENSE-2.0" rel="noopener noreferrer">http://www.apache.org/licenses/LICENSE-2.0</a>)</li>
<li>MIT license
(<a href="https://github.com/06chaynes/http-cache/blob/main/LICENSE-MIT" rel="noopener noreferrer">LICENSE-MIT</a> or <a href="http://opensource.org/licenses/MIT" rel="noopener noreferrer">http://opensource.org/licenses/MIT</a>)</li>
</ul>
<p>at your option.</p>
<h2>Contribution</h2>
<p>Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.</p>

  

        </main>
      </div>

      <footer>
        
          <a href="https://github.com/06chaynes/http-cache"><div class="github-icon" aria-hidden="true"></div></a>
        
        <span>
          http-cache, MIT OR Apache-2.0
        </span>
      </footer>
    </div>

    
    

    
  </body>
</html>