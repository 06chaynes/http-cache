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

          
  
    <h1>http-cache-reqwest</h1>
<p><a href="https://github.com/06chaynes/http-cache/actions/workflows/http-cache-reqwest.yml" rel="noopener noreferrer"><img src="https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-reqwest.yml?label=CI&amp;style=for-the-badge" alt="CI"></a>
<a href="https://crates.io/crates/http-cache-reqwest" rel="noopener noreferrer"><img src="https://img.shields.io/crates/v/http-cache-reqwest?style=for-the-badge" alt="Crates.io"></a>
<a href="https://docs.rs/http-cache-reqwest" rel="noopener noreferrer"><img src="https://img.shields.io/docsrs/http-cache-reqwest?style=for-the-badge" alt="Docs.rs"></a>
<a href="https://app.codecov.io/gh/06chaynes/http-cache" rel="noopener noreferrer"><img src="https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge" alt="Codecov"></a>
<img src="https://img.shields.io/crates/l/http-cache-reqwest?style=for-the-badge" alt="Crates.io"></p>
<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">
<p>A caching middleware that follows HTTP caching rules,
thanks to <a href="https://github.com/kornelski/rusty-http-cache-semantics" rel="noopener noreferrer">http-cache-semantics</a>.
By default, it uses <a href="https://github.com/zkat/cacache-rs" rel="noopener noreferrer">cacache</a> as the backend cache manager.
Uses <a href="https://github.com/TrueLayer/reqwest-middleware" rel="noopener noreferrer">reqwest-middleware</a> for middleware support.</p>
<h2>Minimum Supported Rust Version (MSRV)</h2>
<p>1.82.0</p>
<h2>Install</h2>
<p>With <a href="https://github.com/killercup/cargo-edit#Installation" rel="noopener noreferrer">cargo add</a> installed :</p>
<pre style="background-color:#263238;"><span style="color:#82aaff;">cargo add http-cache-reqwest
</span></pre>

<h2>Example</h2>
<pre style="background-color:#263238;"><span style="color:#c792ea;">use </span><span style="color:#eeffff;">reqwest</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Client</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">reqwest_middleware</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">ClientBuilder</span><span style="color:#89ddff;">, </span><span style="color:#ffcb6b;">Result</span><span style="color:#89ddff;">};
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache_reqwest</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">Cache</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> CacheMode</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> CACacheManager</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> HttpCache</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> HttpCacheOptions</span><span style="color:#89ddff;">};
</span><span style="color:#eeffff;">
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">tokio::main</span><span style="color:#89ddff;">]
</span><span style="color:#eeffff;">async </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">main</span><span style="color:#89ddff;">() -&gt; </span><span style="color:#eeffff;">Result</span><span style="color:#89ddff;">&lt;()&gt; {
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> client </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">ClientBuilder</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Client</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">())
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">with</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Cache</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">HttpCache </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">          mode</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">CacheMode</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Default</span><span style="color:#89ddff;">,
</span><span style="color:#eeffff;">          manager</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">CACacheManager</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">default</span><span style="color:#89ddff;">(),
</span><span style="color:#eeffff;">          options</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">HttpCacheOptions</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">default</span><span style="color:#89ddff;">(),
</span><span style="color:#eeffff;">        </span><span style="color:#89ddff;">}))
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">build</span><span style="color:#89ddff;">();
</span><span style="color:#eeffff;">    client
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">get</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching</span><span style="color:#89ddff;">")
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">send</span><span style="color:#89ddff;">()
</span><span style="color:#eeffff;">        .await</span><span style="color:#89ddff;">?;
</span><span style="color:#eeffff;">    </span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(())
</span><span style="color:#89ddff;">}
</span></pre>

<h2>Streaming Support</h2>
<p>When the <code>streaming</code> feature is enabled, you can use <code>StreamingCache</code> for efficient handling of large responses without buffering them entirely in memory. This provides significant memory savings (typically 35-40% reduction) while maintaining full HTTP caching compliance.</p>
<p><strong>Note</strong>: Only <code>StreamingCacheManager</code> supports streaming. <code>CACacheManager</code> and <code>MokaManager</code> do not support streaming and will buffer responses in memory.</p>
<pre style="background-color:#263238;"><span style="color:#c792ea;">use </span><span style="color:#eeffff;">reqwest</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Client</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">reqwest_middleware</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">ClientBuilder</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache_reqwest</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">StreamingCache</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> CacheMode</span><span style="color:#89ddff;">};
</span><span style="color:#eeffff;">
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">cfg</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">feature </span><span style="color:#89ddff;">= "</span><span style="color:#c3e88d;">streaming</span><span style="color:#89ddff;">")]
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">StreamingCacheManager</span><span style="color:#89ddff;">;
</span><span style="color:#eeffff;">
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">cfg</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">feature </span><span style="color:#89ddff;">= "</span><span style="color:#c3e88d;">streaming</span><span style="color:#89ddff;">")]
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">tokio::main</span><span style="color:#89ddff;">]
</span><span style="color:#eeffff;">async </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">main</span><span style="color:#89ddff;">() -&gt; </span><span style="color:#eeffff;">reqwest_middleware</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Result</span><span style="color:#89ddff;">&lt;()&gt; {
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> client </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">ClientBuilder</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Client</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">())
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">with</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">StreamingCache</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">(
</span><span style="color:#eeffff;">            StreamingCacheManager</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">./cache</span><span style="color:#89ddff;">"</span><span style="color:#eeffff;">.</span><span style="color:#82aaff;">into</span><span style="color:#89ddff;">()),
</span><span style="color:#eeffff;">            CacheMode</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Default</span><span style="color:#89ddff;">,
</span><span style="color:#eeffff;">        </span><span style="color:#89ddff;">))
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">build</span><span style="color:#89ddff;">();
</span><span style="color:#eeffff;">        
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#546e7a;">// Efficiently stream large responses - cached responses are also streamed
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> response </span><span style="color:#89ddff;">=</span><span style="color:#eeffff;"> client
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">get</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">https://httpbin.org/stream/1000</span><span style="color:#89ddff;">")
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">send</span><span style="color:#89ddff;">()
</span><span style="color:#eeffff;">        .await</span><span style="color:#89ddff;">?;
</span><span style="color:#eeffff;">        
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#546e7a;">// Process response as a stream
</span><span style="color:#eeffff;">    </span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">futures_util</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">StreamExt</span><span style="color:#89ddff;">;
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">let </span><span style="color:#c792ea;">mut</span><span style="color:#eeffff;"> stream </span><span style="color:#89ddff;">=</span><span style="color:#eeffff;"> response.</span><span style="color:#82aaff;">bytes_stream</span><span style="color:#89ddff;">();
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">while let </span><span style="color:#ffcb6b;">Some</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">chunk</span><span style="color:#89ddff;">) =</span><span style="color:#eeffff;"> stream.</span><span style="color:#82aaff;">next</span><span style="color:#89ddff;">()</span><span style="color:#eeffff;">.await </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">        </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> chunk </span><span style="color:#89ddff;">=</span><span style="color:#eeffff;"> chunk</span><span style="color:#89ddff;">?;
</span><span style="color:#eeffff;">        </span><span style="font-style:italic;color:#546e7a;">// Process each chunk without loading entire response into memory
</span><span style="color:#eeffff;">        println!</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">Received </span><span style="color:#eeffff;">{}</span><span style="color:#c3e88d;"> bytes</span><span style="color:#89ddff;">",</span><span style="color:#eeffff;"> chunk.</span><span style="color:#82aaff;">len</span><span style="color:#89ddff;">());
</span><span style="color:#eeffff;">    </span><span style="color:#89ddff;">}
</span><span style="color:#eeffff;">    
</span><span style="color:#eeffff;">    </span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(())
</span><span style="color:#89ddff;">}
</span><span style="color:#eeffff;">
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">cfg</span><span style="color:#89ddff;">(</span><span style="color:#82aaff;">not</span><span style="color:#89ddff;">(</span><span style="color:#82aaff;">feature </span><span style="color:#89ddff;">= "</span><span style="color:#c3e88d;">streaming</span><span style="color:#89ddff;">"))]
</span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">main</span><span style="color:#89ddff;">() {}
</span></pre>

<h2>Features</h2>
<p>The following features are available. By default <code>manager-cacache</code> is enabled.</p>
<ul>
<li><code>manager-cacache</code> (default): enable <a href="https://github.com/zkat/cacache-rs" rel="noopener noreferrer">cacache</a>, a high-performance disk cache, backend manager.</li>
<li><code>manager-moka</code> (disabled): enable <a href="https://github.com/moka-rs/moka" rel="noopener noreferrer">moka</a>, a high-performance in-memory cache, backend manager.</li>
<li><code>streaming</code> (disabled): enable streaming cache support with efficient memory usage. Provides <code>StreamingCache</code> middleware that can handle large responses without buffering them entirely in memory, while maintaining full HTTP caching compliance. Requires cache managers that implement <code>StreamingCacheManager</code>.</li>
</ul>
<h2>Documentation</h2>
<ul>
<li><a href="https://docs.rs/http-cache-reqwest" rel="noopener noreferrer">API Docs</a></li>
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