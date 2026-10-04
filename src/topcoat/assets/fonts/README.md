# Embedded application fonts

These unmodified WOFF2 files reproduce the existing production Google Fonts request locally. Retrieved 2026-10-04 using Chrome 130 Linux user agent:

```
https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=DM+Sans:ital,opsz,wght@0,9..40,300..700;1,9..40,300..700&display=swap
```

`google.css` preserves the returned stylesheet. Application `base.css` uses identical declarations with relative local URLs. All returned subsets are retained: DM Sans normal/italic Latin and Latin-ext; Space Grotesk Latin, Latin-ext and Vietnamese. DM Sans declarations retain weight 300–700 and the requested optical-size range; Space Grotesk retains declarations for 400/500/600/700, sharing the upstream variable bytes. No conversion or subsetting was performed. `font-display: swap` and Unicode ranges are unchanged.

Both families are licensed under SIL Open Font License 1.1. Official license sources:

- https://raw.githubusercontent.com/google/fonts/main/ofl/dmsans/OFL.txt
- https://raw.githubusercontent.com/google/fonts/main/ofl/spacegrotesk/OFL.txt

## Exact asset provenance

- `dm-sans-italic-latin-ext.woff2`
  - Source: https://fonts.gstatic.com/s/dmsans/v17/rP2Fp2ywxg089UriCZa4ET-DNl0.woff2
  - SHA256: `b72f7226650dcbc66e1e6ccdc9e53dc8c66107357eeac2d2a4ca18942c42f360`
- `dm-sans-italic-latin.woff2`
  - Source: https://fonts.gstatic.com/s/dmsans/v17/rP2Fp2ywxg089UriCZa4Hz-D.woff2
  - SHA256: `d5c53a50536536971ea27318a590dbf723a190dd2f608e7a92929a021cc0ebaa`
- `dm-sans-normal-latin-ext.woff2`
  - Source: https://fonts.gstatic.com/s/dmsans/v17/rP2Hp2ywxg089UriCZ2IHSeH.woff2
  - SHA256: `5d18d31d23ada61ebee1d589b11d5da30db9e158f589d5e35221ba2b1a45de54`
- `dm-sans-normal-latin.woff2`
  - Source: https://fonts.gstatic.com/s/dmsans/v17/rP2Hp2ywxg089UriCZOIHQ.woff2
  - SHA256: `ca72d2bcea8f4daa783dbdfa2d9b46068c3ce38168e05918fb867aa453b4f890`
- `space-grotesk-normal-vietnamese.woff2`
  - Source: https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPb54C-s0.woff2
  - SHA256: `8895e3d49825128bfa5238e4c96e9b432d7a3dadbe5c17c6081267398305dc71`
- `space-grotesk-normal-latin-ext.woff2`
  - Source: https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPb94C-s0.woff2
  - SHA256: `952dddb45d2f96f71cbf3b7f510b24379afc3c89ea02fcf89d377b45d62c0166`
- `space-grotesk-normal-latin.woff2`
  - Source: https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPbF4Cw.woff2
  - SHA256: `0640890476fc1198ab4de571fb658de443c4d85b66466ec09534a8737ab1ce9d`

## License and stylesheet checksums

- `google.css`: `925c2de4cc33e2f391a588554333ddc178cdab61bd4ff5491b80d01cb1383b70`
- `dmsans-OFL.txt`: `9af36190332437f5ecd09974de43c1f7c77a310a996cdd8ceb25628b458840e1`
- `spacegrotesk-OFL.txt`: `564ce565c371c5e5bbf286006565a7c9aa55a9f56e7ca58d56e05d649dd61a72`
