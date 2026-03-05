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

          
  
    <h1>http-cache-quickcache</h1>
<p><a href="https://github.com/06chaynes/http-cache/actions/workflows/http-cache-quickcache.yml" rel="noopener noreferrer"><img src="https://img.shields.io/github/actions/workflow/status/06chaynes/http-cache/http-cache-quickcache.yml?label=CI&amp;style=for-the-badge" alt="CI"></a>
<a href="https://crates.io/crates/http-cache-quickcache" rel="noopener noreferrer"><img src="https://img.shields.io/crates/v/http-cache-quickcache?style=for-the-badge" alt="Crates.io"></a>
<a href="https://docs.rs/http-cache-quickcache" rel="noopener noreferrer"><img src="https://img.shields.io/docsrs/http-cache-quickcache?style=for-the-badge" alt="Docs.rs"></a>
<a href="https://app.codecov.io/gh/06chaynes/http-cache" rel="noopener noreferrer"><img src="https://img.shields.io/codecov/c/github/06chaynes/http-cache?style=for-the-badge" alt="Codecov"></a>
<img src="https://img.shields.io/crates/l/http-cache-quickcache?style=for-the-badge" alt="Crates.io"></p>
<img class="logo" align="right" src="https://raw.githubusercontent.com/06chaynes/http-cache/main/.assets/images/http-cache_logo_bluegreen.svg" height="150px" alt="the http-cache logo">
<p>An http-cache manager implementation for <a href="https://github.com/arthurprs/quick-cache" rel="noopener noreferrer">quick-cache</a>.</p>
<h2>Minimum Supported Rust Version (MSRV)</h2>
<p>1.88.0</p>
<h2>Install</h2>
<p>With <a href="https://github.com/killercup/cargo-edit#Installation" rel="noopener noreferrer">cargo add</a> installed :</p>
<pre style="background-color:#263238;"><span style="color:#82aaff;">cargo add http-cache-quickcache
</span></pre>

<h2>Example</h2>
<h3>With Tower Services</h3>
<pre style="background-color:#263238;"><span style="color:#c792ea;">use </span><span style="color:#eeffff;">tower</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">Service</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> ServiceExt</span><span style="color:#89ddff;">};
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">Request</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> Response</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> StatusCode</span><span style="color:#89ddff;">};
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_body_util</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">bytes</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache_quickcache</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">QuickManager</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">convert</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Infallible</span><span style="color:#89ddff;">;
</span><span style="color:#eeffff;">
</span><span style="font-style:italic;color:#546e7a;">// Example Tower service that uses QuickManager for caching
</span><span style="color:#89ddff;">#[</span><span style="color:#eeffff;">derive</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Clone</span><span style="color:#89ddff;">)]
</span><span style="font-style:italic;color:#c792ea;">struct </span><span style="color:#eeffff;">CachingService </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">    cache_manager</span><span style="color:#89ddff;">:</span><span style="color:#eeffff;"> QuickManager,
</span><span style="color:#89ddff;">}
</span><span style="color:#eeffff;">
</span><span style="font-style:italic;color:#c792ea;">impl </span><span style="color:#eeffff;">Service</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Request</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">&gt;&gt;&gt; </span><span style="color:#c792ea;">for </span><span style="color:#eeffff;">CachingService </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">type </span><span style="color:#eeffff;">Response </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">Response</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">&gt;&gt;;
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">type </span><span style="color:#eeffff;">Error </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">Box</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">dyn std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">error</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Error </span><span style="color:#89ddff;">+</span><span style="color:#eeffff;"> Send </span><span style="color:#89ddff;">+</span><span style="color:#eeffff;"> Sync</span><span style="color:#89ddff;">&gt;;
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">type </span><span style="color:#eeffff;">Future </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">pin</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Pin</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Box</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">dyn std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">future</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Future</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Output = Result</span><span style="color:#89ddff;">&lt;</span><span style="font-style:italic;color:#c792ea;">Self</span><span style="font-style:italic;color:#89ddff;">::</span><span style="color:#eeffff;">Response, </span><span style="font-style:italic;color:#c792ea;">Self</span><span style="font-style:italic;color:#89ddff;">::</span><span style="color:#eeffff;">Error</span><span style="color:#89ddff;">&gt;&gt; +</span><span style="color:#eeffff;"> Send</span><span style="color:#89ddff;">&gt;&gt;;
</span><span style="color:#eeffff;">
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">poll_ready</span><span style="color:#89ddff;">(&amp;</span><span style="color:#c792ea;">mut </span><span style="color:#f78c6c;">self</span><span style="color:#eeffff;">, </span><span style="color:#f78c6c;">_cx</span><span style="color:#89ddff;">: &amp;</span><span style="color:#c792ea;">mut </span><span style="color:#eeffff;">std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">task</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Context</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">'</span><span style="color:#89ddff;">_&gt;) -&gt; </span><span style="color:#eeffff;">std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">task</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Poll</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Result</span><span style="color:#89ddff;">&lt;()</span><span style="color:#eeffff;">, </span><span style="font-style:italic;color:#c792ea;">Self</span><span style="font-style:italic;color:#89ddff;">::</span><span style="color:#eeffff;">Error</span><span style="color:#89ddff;">&gt;&gt; {
</span><span style="color:#eeffff;">        std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">task</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Poll</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Ready</span><span style="color:#89ddff;">(</span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(()))
</span><span style="color:#eeffff;">    </span><span style="color:#89ddff;">}
</span><span style="color:#eeffff;">
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">call</span><span style="color:#89ddff;">(&amp;</span><span style="color:#c792ea;">mut </span><span style="color:#f78c6c;">self</span><span style="color:#eeffff;">, </span><span style="color:#f78c6c;">req</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">Request</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">&gt;&gt;) -&gt; </span><span style="font-style:italic;color:#c792ea;">Self</span><span style="font-style:italic;color:#89ddff;">::</span><span style="color:#eeffff;">Future </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">        </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> manager </span><span style="color:#89ddff;">= </span><span style="font-style:italic;color:#ff5370;">self</span><span style="color:#eeffff;">.cache_manager.</span><span style="color:#82aaff;">clone</span><span style="color:#89ddff;">();
</span><span style="color:#eeffff;">        </span><span style="color:#ffcb6b;">Box</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">pin</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">async </span><span style="color:#c792ea;">move </span><span style="color:#89ddff;">{
</span><span style="color:#eeffff;">            </span><span style="font-style:italic;color:#546e7a;">// Cache logic using the manager would go here
</span><span style="color:#eeffff;">            </span><span style="font-style:italic;color:#c792ea;">let</span><span style="color:#eeffff;"> response </span><span style="color:#89ddff;">= </span><span style="color:#eeffff;">Response</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">builder</span><span style="color:#89ddff;">()
</span><span style="color:#eeffff;">                .</span><span style="color:#82aaff;">status</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">StatusCode</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">OK</span><span style="color:#89ddff;">)
</span><span style="color:#eeffff;">                .</span><span style="color:#82aaff;">body</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">from</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">Hello from cached service!</span><span style="color:#89ddff;">")))?;
</span><span style="color:#eeffff;">            </span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">response</span><span style="color:#89ddff;">)
</span><span style="color:#eeffff;">        </span><span style="color:#89ddff;">})
</span><span style="color:#eeffff;">    </span><span style="color:#89ddff;">}
</span><span style="color:#89ddff;">}
</span></pre>

<h3>With Hyper</h3>
<pre style="background-color:#263238;"><span style="color:#c792ea;">use </span><span style="color:#eeffff;">hyper</span><span style="color:#89ddff;">::{</span><span style="color:#eeffff;">Request</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> Response</span><span style="color:#89ddff;">,</span><span style="color:#eeffff;"> StatusCode</span><span style="color:#89ddff;">, </span><span style="color:#eeffff;">body</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Incoming</span><span style="color:#89ddff;">};
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_body_util</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">bytes</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">http_cache_quickcache</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">QuickManager</span><span style="color:#89ddff;">;
</span><span style="color:#c792ea;">use </span><span style="color:#eeffff;">std</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">convert</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">Infallible</span><span style="color:#89ddff;">;
</span><span style="color:#eeffff;">
</span><span style="color:#eeffff;">async </span><span style="font-style:italic;color:#c792ea;">fn </span><span style="color:#82aaff;">handle_request</span><span style="color:#89ddff;">(
</span><span style="color:#eeffff;">    </span><span style="color:#f78c6c;">_req</span><span style="color:#89ddff;">: </span><span style="color:#eeffff;">Request</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Incoming</span><span style="color:#89ddff;">&gt;</span><span style="color:#eeffff;">,
</span><span style="color:#eeffff;">    </span><span style="color:#f78c6c;">cache_manager</span><span style="color:#89ddff;">:</span><span style="color:#eeffff;"> QuickManager,
</span><span style="color:#89ddff;">) -&gt; </span><span style="color:#eeffff;">Result</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Response</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">&lt;</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">&gt;&gt;</span><span style="color:#eeffff;">, Infallible</span><span style="color:#89ddff;">&gt; {
</span><span style="color:#eeffff;">    </span><span style="font-style:italic;color:#546e7a;">// Use cache_manager here for caching responses
</span><span style="color:#eeffff;">    </span><span style="color:#ffcb6b;">Ok</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Response</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">builder</span><span style="color:#89ddff;">()
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">status</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">StatusCode</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">OK</span><span style="color:#89ddff;">)
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">header</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">cache-control</span><span style="color:#89ddff;">", "</span><span style="color:#c3e88d;">max-age=3600</span><span style="color:#89ddff;">")
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">body</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Full</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">new</span><span style="color:#89ddff;">(</span><span style="color:#eeffff;">Bytes</span><span style="color:#89ddff;">::</span><span style="color:#eeffff;">from</span><span style="color:#89ddff;">("</span><span style="color:#c3e88d;">Hello from Hyper with caching!</span><span style="color:#89ddff;">")))
</span><span style="color:#eeffff;">        .</span><span style="color:#82aaff;">unwrap</span><span style="color:#89ddff;">())
</span><span style="color:#89ddff;">}
</span></pre>

<h2>Documentation</h2>
<ul>
<li><a href="https://docs.rs/http-cache-quickcache" rel="noopener noreferrer">API Docs</a></li>
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