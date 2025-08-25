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
<h2>[1.0.0-alpha.2] - 2025-08-24</h2>
<h3>Added</h3>
<ul>
<li>Support for cache-aware rate limiting through <code>rate_limiter</code> field in <code>HttpCacheOptions</code></li>
<li>New <code>rate-limiting</code> feature flag for optional rate limiting functionality</li>
<li>Re-export of rate limiting types: <code>CacheAwareRateLimiter</code>, <code>DomainRateLimiter</code>, <code>DirectRateLimiter</code>, <code>Quota</code></li>
<li>Rate limiting integration for streaming cache operations via <code>StreamingCache</code> middleware</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Consolidated error handling: removed separate error module and replaced with type alias <code>pub use http_cache::{BadRequest, HttpCacheError};</code></li>
<li>Simplified error architecture by removing duplicate error implementations</li>
<li>Standardized error types to follow <code>{CrateName}Error</code> naming convention</li>
</ul>
<h3>Fixed</h3>
<ul>
<li>Request cloning failures for multipart forms and streaming bodies now fallback gracefully instead of throwing errors</li>
<li>Cache middleware now bypasses caching for non-cloneable requests while still performing cache maintenance operations</li>
</ul>
<h2>[1.0.0-alpha.1] - 2025-07-27</h2>
<h3>Added</h3>
<ul>
<li>Support for streaming cache architecture with new <code>streaming</code> features</li>
<li>Integration with <code>HttpCacheStreamInterface</code> for composable streaming middleware</li>
<li>New streaming examples: <code>reqwest_streaming.rs</code> and <code>streaming_memory_profile.rs</code></li>
<li>Enhanced error handling and conditional request support</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated to use http-cache 1.0.0-alpha.1 with streaming support</li>
<li>MSRV updated to 1.82.0</li>
</ul>
<h2>[0.16.0] - 2025-06-25</h2>
<h3>Added</h3>
<ul>
<li><code>remove_opts</code> field to <code>CACacheManager</code> struct. This field is an instance of <code>cacache::RemoveOpts</code> that allows for customization of the removal options when deleting items from the cache.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.82.0</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.21.0]</li>
</ul>
</li>
</ul>
<h2>[0.15.1] - 2025-01-30</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.20.1]</li>
<li>anyhow [1.0.95]</li>
<li>async-trait [0.1.85]</li>
<li>http [1.2.0]</li>
<li>reqwest [0.12.12]</li>
<li>serde [1.0.217]</li>
<li>url [2.5.4]</li>
</ul>
</li>
</ul>
<h2>[0.15.0] - 2024-11-12</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.20.0]</li>
<li>reqwest-middleware [0.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.14.0] - 2024-04-10</h2>
<h3>Changed</h3>
<ul>
<li>
<p>Replaced <code>task-local-extensions::Extensions</code> with <code>http::Extensions</code>.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.19.0]</li>
<li>http-cache-semantics [2.1.0]</li>
<li>http [1.1.0]</li>
<li>reqwest [0.12.3]</li>
<li>reqwest-middleware [0.3.0]</li>
</ul>
</li>
</ul>
<h2>[0.13.0] - 2024-01-15</h2>
<h3>Added</h3>
<ul>
<li>Implemented <code>overridden_cache_mode</code> method from <code>Middleware</code> trait. This method allows for overriding any cache mode set in the configuration, including <code>cache_mode_fn</code>.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.18.0]</li>
</ul>
</li>
</ul>
<h2>[0.12.0] - 2023-11-01</h2>
<h3>Added</h3>
<ul>
<li>The following fields to <code>HttpCacheOptions</code> struct:
<ul>
<li><code>cache_mode_fn</code> field. This is a closure that takes a <code>&amp;http::request::Parts</code> and returns a <code>CacheMode</code> enum variant. This allows for the overriding of cache mode on a per-request basis.</li>
<li><code>cache_bust</code> field. This is a closure that takes <code>http::request::Parts</code>, <code>Option&lt;CacheKey&gt;</code>, the default cache key (<code>&amp;str</code>) and returns <code>Vec&lt;String&gt;</code> of keys to bust the cache for.</li>
</ul>
</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.17.0]</li>
</ul>
</li>
</ul>
<h2>[0.11.3] - 2023-09-28</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.67.1</p>
</li>
<li>
<p>Implemented check to determine if a request is cacheable before running, avoiding the core logic if not.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.16.0]</li>
</ul>
</li>
</ul>
<h2>[0.11.2] - 2023-09-26</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.15.0]</li>
</ul>
</li>
</ul>
<h2>[0.11.1] - 2023-07-28</h2>
<h3>Changed</h3>
<ul>
<li>
<p>Using new <code>cacache-tokio</code> feature in <code>http-cache</code> dependency</p>
</li>
<li>
<p>Exporting <code>CacheManager</code> trait</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.14.0]</li>
<li>async-trait [0.1.72]</li>
<li>serde [1.0.178]</li>
</ul>
</li>
</ul>
<h2>[0.11.0] - 2023-07-19</h2>
<h3>Changed</h3>
<ul>
<li>
<p>Implemented new <code>HttpCacheOptions</code> struct</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.13.0]</li>
<li>anyhow [1.0.72]</li>
<li>async-trait [0.1.71]</li>
<li>serde [1.0.171]</li>
<li>tokio [1.29.1]</li>
</ul>
</li>
</ul>
<h2>[0.10.0] - 2022-06-05</h2>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.66.1</li>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.12.0]</li>
<li>anyhow [1.0.71]</li>
<li>reqwest [0.11.18]</li>
<li>reqwest-middleware [0.2.2]</li>
<li>serde [1.0.163]</li>
<li>tokio [1.28.2]</li>
<li>url [2.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.9.0] - 2022-03-29</h2>
<h3>Added</h3>
<ul>
<li><code>BadRequest</code> error type for request parsing failure.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.11.0]</li>
<li>anyhow [1.0.70]</li>
<li>async-trait [0.1.68]</li>
<li>reqwest [0.11.16]</li>
<li>reqwest-middleware [0.2.1]</li>
<li>serde [1.0.159]</li>
<li>task-local-extensions [0.1.4]</li>
<li>tokio [1.27.0]</li>
</ul>
</li>
</ul>
<h2>[0.8.0] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.63.0</p>
</li>
<li>
<p>Set <code>default-features = false</code> for <code>http-cache</code> dependency</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.10.1]</li>
<li>async-trait [0.1.66]</li>
<li>serde [1.0.154]</li>
<li>tokio [1.26.0]</li>
</ul>
</li>
</ul>
<h2>[0.7.2] - 2023-02-23</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.9.2]</li>
</ul>
</li>
</ul>
<h2>[0.7.1] - 2023-02-17</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.9.1]</li>
<li>http [0.2.9]</li>
</ul>
</li>
</ul>
<h2>[0.7.0] - 2023-02-16</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.62.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.9.0]</li>
</ul>
</li>
</ul>
<h2>[0.6.0] - 2023-02-07</h2>
<ul>
<li>MSRV is now 1.60.0</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.8.0]</li>
<li>anyhow [1.0.69]</li>
<li>async-trait [0.1.64]</li>
<li>reqwest [0.11.14]</li>
<li>serde [1.0.152]</li>
<li>tokio [1.25.0]</li>
</ul>
</li>
</ul>
<h2>[0.5.2] - 2022-11-16</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.7.2]</li>
<li>reqwest [0.11.13]</li>
<li>reqwest-middleware [0.2.0]</li>
</ul>
</li>
</ul>
<h2>[0.5.1] - 2022-11-06</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.7.1]</li>
<li>anyhow [1.0.66]</li>
<li>async-trait [0.1.58]</li>
<li>reqwest [0.11.12]</li>
<li>serde [1.0.147]</li>
<li>url [2.3.1]</li>
<li>task-local-extensions [0.1.3]</li>
<li>tokio [1.21.2]</li>
</ul>
</li>
</ul>
<h2>[0.5.0] - 2022-06-17</h2>
<h3>Changed</h3>
<ul>
<li>
<p>The <code>CacheManager</code> trait is now implemented directly against the <code>MokaManager</code> struct rather than <code>Arc&lt;MokaManager&gt;</code>. The Arc is now internal to the <code>MokaManager</code> struct as part of the <code>cache</code> field.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.7.0]</li>
<li>async-trait [0.1.56]</li>
<li>http [0.2.8]</li>
<li>reqwest [0.11.11]</li>
<li>serde [1.0.137]</li>
<li>tokio [1.19.2]</li>
</ul>
</li>
</ul>
<h2>[0.4.5] - 2022-04-30</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.6.5]</li>
<li>http [0.2.7]</li>
<li>tokio [1.18.0]</li>
</ul>
</li>
</ul>
<h2>[0.4.4] - 2022-04-26</h2>
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