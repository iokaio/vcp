VCP setup runtime notices
=========================

The VCP installer is made using Inno Setup 6.7.3 by Jordan Russell and
Martijn Laan: https://jrsoftware.org/. It is also made using RemObjects
Pascal Script by Carlo Kok and RemObjects Software:
https://www.remobjects.com/ps.aspx . See Inno-Setup.txt and
RemObjects-Pascal-Script.txt for the preserved licenses.

These notices cover the setup/uninstall runtime. Native engine and VSIX
components are inventoried separately in their own payloads. VCP does not
modify the upstream Inno executable runtime. Its installer scripts are VCP
code; the upstream runtime and compiler are supplied by the exact published
Inno installer whose hash is pinned in the VCP release channel.

Upstream source revision:
https://github.com/jrsoftware/issrc/tree/4adf37ed7f3fd2bd11c6836ba056e3de170fbabf

License choices and corresponding source
--------------------------------------

Inno's NewUxTheme.pas and NewUxTheme.TmSchema.pas offer MPL 1.1 or LGPL.
VCP selects MPL 1.1. The covered source is provided under that license in
sources/theme/ on the same distribution media, with the original notices
and upstream modification descriptions intact. See MPL-1.1.txt. These files
are copied unchanged from the pinned Inno revision; VCP made no changes.

The SetupLdr small LZMA decoder identifies LZMA SDK 4.40 and offers CPL or
LGPL, with an explicit linking exception. VCP selects CPL 1.0 with that
exception. See CPL-1.0.html and the original exception in the source headers.
The exact decoder source, headers and upstream compile script are included
in sources/lzma-loader/. These files are unchanged from the pinned Inno
revision; upstream's additional helper functions remain plainly identified.
The rights and obligations for this source are those in its own license;
VCP does not impose additional restrictions on that source.

The newer LZMA SDK/7z ANSI-C decoding code is placed in the public domain
by Igor Pavlov; see LZMA-SDK.txt. The pinned Components/Lzma2/7zVersion.h
identifies its SDK version as 25.01. Inno's MD5/SHA1/SHA256 wrappers also
declare public-domain status in their source headers.

This setup selects LZMA2 compression and basic archive extraction. It does
not include the optional is7z full-archive extraction DLL or the compiler,
IDE, Scintilla editor or separate bzip/zlib compressor DLLs as product files.

Provenance limits
-----------------

The prebuilt Inno runtime contains its upstream Delphi runtime dependencies.
VCP uses the upstream redistribution permission and preserves supplied
copyright resources. VCP does not rebuild Inno, independently attest to its
Delphi build, or claim that the published binary is reproducible from source.
The setup receipt records the pinned installer and extracted compiler/runtime
file hashes; inventory.json binds the notice/source copies shipped here.
