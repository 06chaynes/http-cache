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

          
  
    <h1>Changelog</h1>
<h2>[0.21.0] - 2025-06-25</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>remove_opts</code> field to <code>CACacheManager</code> struct. This field is an instance of <code>cacache::RemoveOpts</code> that allows for customization of the removal options when deleting items from the cache.</p>
</li>
<li>
<p>MSRV is now 1.82.0</p>
</li>
</ul>
<h2>[0.20.1] - 2025-01-30</h2>
<h3>Changed</h3>
<ul>
<li>
<p>Fixed missing implementation of CacheMode::Reload variant logic.</p>
</li>
<li>
<p>MSRV is now 1.81.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.85]</li>
<li>cacache [13.1.0]</li>
<li>httpdate [1.0.2]</li>
<li>moka [0.12.10]</li>
<li>serde [1.0.217]</li>
<li>url [2.5.4]</li>
</ul>
</li>
</ul>
<h2>[0.20.0] - 2024-11-12</h2>
<h3>Added</h3>
<ul>
<li><code>cache_status_headers</code> field to <code>HttpCacheOptions</code> struct. This field is a boolean that determines if the cache status headers should be added to the response.</li>
</ul>
<h2>[0.19.0] - 2024-04-10</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [13.0.0]</li>
<li>http [1.1.0]</li>
<li>http-cache-semantics [2.1.0]</li>
</ul>
</li>
</ul>
<h2>[0.18.0] - 2024-01-15</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>overridden_cache_mode</code> method to <code>Middleware</code> trait. This method allows for overriding any cache mode set in the configuration, including <code>cache_mode_fn</code>.</p>
</li>
<li>
<p>Derive <code>Default</code> for the <code>CacheMode</code> enum with the mode <code>Default</code> selected to be used.</p>
</li>
</ul>
<h2>[0.17.0] - 2023-11-01</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>cache_mode_fn</code> field to <code>HttpCacheOptions</code> struct. This is a closure that takes a <code>&amp;http::request::Parts</code> and returns a <code>CacheMode</code> enum variant. This allows for the overriding of cache mode on a per-request basis.</p>
</li>
<li>
<p><code>cache_bust</code> field to <code>HttpCacheOptions</code> struct. This is a closure that takes <code>http::request::Parts</code>, <code>Option&lt;CacheKey&gt;</code>, the default cache key (<code>&amp;str</code>) and returns <code>Vec&lt;String&gt;</code> of keys to bust the cache for.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [12.0.0]</li>
</ul>
</li>
</ul>
<h2>[0.16.0] - 2023-09-28</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>can_cache_request</code> method to <code>HttpCache</code> struct. This can be used by client implementations to determine if the request should be cached.</p>
</li>
<li>
<p><code>run_no_cache</code> method to <code>HttpCache</code> struct. This should be run by client implementations if the request is determined to not be cached.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.67.1</li>
</ul>
<h2>[0.15.0] - 2023-09-26</h2>
<h3>Added</h3>
<ul>
<li><code>IgnoreRules</code> variant to the <code>CacheMode</code> enum. This mode will ignore the HTTP headers and always store a response given it was a 200 response. It will also ignore the staleness when retrieving a response from the cache, so expiration of the cached response will need to be handled manually. If there was no cached response it will create a normal request, and will update the cache with the response.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>moka [0.12.0]</li>
</ul>
</li>
</ul>
<h2>[0.14.0] - 2023-07-28</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>cacache-async-std</code> feature, which enables <code>async_std</code> runtime support in the <code>cacache</code> backend manager. This feature is enabled by default.</p>
</li>
<li>
<p><code>cacache-tokio</code> feature, which enables <code>tokio</code> runtime support in the <code>cacache</code> backend manager. This feature is disabled by default.</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>async-std [1.12.0]</li>
<li>async-trait [0.1.72]</li>
<li>serde [1.0.178]</li>
<li>tokio [1.29.1]</li>
</ul>
</li>
</ul>
<h2>[0.13.0] - 2023-07-19</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>CacheKey</code> type, a closure that takes [<code>http::request::Parts</code>] and returns a [<code>String</code>].</p>
</li>
<li>
<p><code>HttpCacheOptions</code> struct that contains the cache key (<code>CacheKey</code>) and the cache options (<code>CacheOptions</code>).</p>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>
<p><code>CacheManager</code> trait <code>get</code>, <code>put</code>, and <code>delete</code> methods now require a <code>cache_key</code> argument rather than <code>method</code> and <code>url</code> arguments. This allows for custom keys to be specified.</p>
</li>
<li>
<p>Both the <code>CACacheManager</code> trait and <code>MokaManager</code> implementation have been updated to reflect the above change.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.71]</li>
<li>moka [0.11.2]</li>
<li>serde [1.0.171]</li>
</ul>
</li>
</ul>
<h2>[0.12.0] - 2023-06-05</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.66.1</p>
</li>
<li>
<p><code>CACacheManager</code> field <code>path</code> has changed to <code>std::path::PathBuf</code></p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>cacache [11.6.0]</li>
<li>moka [0.11.1]</li>
<li>serde [1.0.163]</li>
<li>url [2.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.11.0] - 2023-03-29</h2>
<h3>Added</h3>
<ul>
<li>
<p><code>BoxError</code> type alias for <code>Box&lt;dyn std::error::Error + Send + Sync&gt;</code>.</p>
</li>
<li>
<p><code>BadVersion</code> error type for unknown http versions.</p>
</li>
<li>
<p><code>BadHeader</code> error type for bad http header values.</p>
</li>
</ul>
<h3>Removed</h3>
<ul>
<li>
<p><code>CacheError</code> enum.</p>
</li>
<li>
<p>The following dependencies:</p>
<ul>
<li>anyhow</li>
<li>thiserror</li>
<li>miette</li>
</ul>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>
<p><code>CacheError</code> enum has been replaced in function by <code>Box&lt;dyn std::error::Error + Send + Sync&gt;</code>.</p>
</li>
<li>
<p><code>Result</code> typedef is now <code>std::result::Result&lt;T, BoxError&gt;</code>.</p>
</li>
<li>
<p><code>Error</code> type for the TryFrom implentation for the <code>HttpVersion</code> struct is now <code>BoxError</code> containing a <code>BadVersion</code> error.</p>
</li>
<li>
<p><code>CacheManager</code> trait <code>put</code> method now returns <code>Result&lt;(), BoxError&gt;</code>.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.68]</li>
<li>cacache [11.4.0]</li>
<li>moka [0.10.1]</li>
<li>serde [1.0.159]</li>
</ul>
</li>
</ul>
<h2>[0.10.1] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>Set conditional check for <code>CacheError::Bincode</code> to <code>cfg(feature = "bincode")</code></li>
</ul>
<h2>[0.10.0] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.63.0</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.66]</li>
<li>cacache [11.3.0]</li>
<li>serde [1.0.154]</li>
<li>thiserror [1.0.39]</li>
</ul>
</li>
</ul>
<h2>[0.9.2] - 2023-02-23</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>cacache [11.1.0]</li>
</ul>
</li>
</ul>
<h2>[0.9.1] - 2023-02-17</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http [0.2.9]</li>
</ul>
</li>
</ul>
<h2>[0.9.0] - 2023-02-16</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.62.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>moka [0.10.0]</li>
</ul>
</li>
</ul>
<h2>[0.8.0] - 2023-02-07</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.60.0</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>anyhow [1.0.69]</li>
<li>async-trait [0.1.64]</li>
<li>cacache [11.0.0]</li>
<li>miette [5.5.0]</li>
<li>moka [0.9.7]</li>
<li>serde [1.0.152]</li>
<li>thiserror [1.0.38]</li>
</ul>
</li>
</ul>
<h2>[0.7.2] - 2022-11-16</h2>
<ul>
<li>Added derive <code>Eq</code> to <code>HttpVersion</code> enum.</li>
</ul>
<h3>Changed</h3>
<h2>[0.7.1] - 2022-11-06</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>anyhow [1.0.66]</li>
<li>async-trait [0.1.58]</li>
<li>miette [5.4.1]</li>
<li>moka [0.9.6]</li>
<li>serde [1.0.147]</li>
<li>thiserror [1.0.37]</li>
<li>url [2.3.1]</li>
</ul>
</li>
</ul>
<h2>[0.7.0] - 2022-06-17</h2>
<h3>Changed</h3>
<ul>
<li>
<p>The <code>CacheManager</code> trait is now implemented directly against the <code>MokaManager</code> struct rather than <code>Arc&lt;MokaManager&gt;</code>. The Arc is now internal to the <code>MokaManager</code> struct as part of the <code>cache</code> field.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>async-trait [0.1.56]</li>
<li>http [0.2.8]</li>
<li>miette [4.7.1]</li>
<li>moka [0.8.5]</li>
<li>serde [1.0.137]</li>
<li>thiserror [1.0.31]</li>
</ul>
</li>
</ul>
<h2>[0.6.5] - 2022-04-30</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http [0.2.7]</li>
</ul>
</li>
</ul>
<h2>[0.6.4] - 2022-04-26</h2>
<h3>Added</h3>
<ul>
<li>This changelog to keep a record of notable changes to the project.</li>
</ul>

  

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