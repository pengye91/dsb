// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useState, useEffect, useRef } from 'react';
import {
  Box,
  VStack,
  HStack,
  Text,
  Spinner,
  useColorModeValue,
  Divider,
  IconButton,
  Tooltip,
  Button,
  ButtonGroup,
} from '@chakra-ui/react';
import {
  File,
  Folder,
  FolderOpen,
  ChevronRight,
  ChevronDown,
  ChevronsLeft,
  ChevronsRight,
  RefreshCw,
  Download,
  FileText,
  FileCode,
  Image as ImageIcon,
  Film,
  Music,
  Archive,
  FileJson,
  Eye,
  Code,
  ExternalLink,
} from 'lucide-react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus, vs } from 'react-syntax-highlighter/dist/esm/styles/prism';
import { apiClient } from '../../api/client';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import DOMPurify from 'dompurify';

const API_BASE_PATH = import.meta.env.VITE_BASE_PATH || '';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { usePanelSize } from '../../hooks/usePanelSize';
import { MiniFsResizeHandle } from './MiniFsResizeHandle';

interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  size?: number;
  children?: FileNode[];
}

interface DirectoryTreeResponse {
  sandbox_id: string;
  tree: FileNode[];
}

interface MiniFsProps {
  sandboxId: string;
}

// Panel size constraints
const PANEL_SIZE_CONSTRAINTS = {
  /** Minimum panel size percentage */
  MIN_SIZE: 20,
  /** Maximum panel size percentage */
  MAX_SIZE: 80,
  /** Threshold below which panel is considered collapsed */
  COLLAPSED_THRESHOLD: 25,
  /** Panel size when collapsed */
  COLLAPSED_SIZE: 20,
  /** Default panel size when expanded */
  DEFAULT_EXPANDED_SIZE: 40,
} as const;

export function MiniFs({ sandboxId }: MiniFsProps) {
  const [tree, setTree] = useState<FileNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<FileNode | null>(null);
  const [fileContent, setFileContent] = useState<string>('');
  const [loadingContent, setLoadingContent] = useState(false);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<'raw' | 'preview'>('raw');
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [isLeftPanelCollapsed, setIsLeftPanelCollapsed] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const { leftPanelSize, setLeftPanelSize, resetPanelSize, isLoaded: isPanelSizeLoaded } = usePanelSize();

  // Panel refs for imperative API
  const leftPanelRef = useRef<any>(null);
  const rightPanelRef = useRef<any>(null);

  const bgColor = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');
  const hoverBg = useColorModeValue('gray.50', 'gray.700');
  const treeBg = useColorModeValue('gray.50', 'gray.900');

  // Collapse panel using imperative API
  const handleCollapse = () => {
    if (leftPanelRef.current) {
      leftPanelRef.current.collapse();
    }
    setIsLeftPanelCollapsed(true);
  };

  // Expand panel using imperative API
  const handleExpand = () => {
    if (leftPanelRef.current) {
      leftPanelRef.current.expand();
    }
    setIsLeftPanelCollapsed(false);
  };

  const loadDirectoryTree = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(
        `${API_BASE_PATH}/static/tree/${sandboxId}`,
        {
          headers: {
            'X-API-Key': apiClient.getApiKey() || '',
          },
        }
      );
      if (!response.ok) {
        throw new Error('Failed to load directory tree');
      }
      const data: DirectoryTreeResponse = await response.json();
      setTree(data.tree);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const loadFileContent = async (file: FileNode) => {
    if (file.is_dir) return;

    setLoadingContent(true);
    setPreviewError(null);
    try {
      const response = await fetch(
        `${API_BASE_PATH}/static/${sandboxId}/${file.path}`,
        {
          headers: {
            'X-API-Key': apiClient.getApiKey() || '',
          },
        }
      );
      if (!response.ok) {
        throw new Error('Failed to load file content');
      }
      const text = await response.text();
      setFileContent(text);
      resetViewMode(file);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoadingContent(false);
    }
  };

  const downloadAllFiles = async () => {
    setDownloading(true);
    setError(null);
    try {
      const response = await fetch(
        `${API_BASE_PATH}/static/download/${sandboxId}`,
        {
          headers: {
            'X-API-Key': apiClient.getApiKey() || '',
          },
        }
      );

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || 'Failed to download files');
      }

      // Get the blob from response
      const blob = await response.blob();

      // Create download link
      const url = window.URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `sandbox-${sandboxId}-files.zip`;

      // Trigger download
      document.body.appendChild(link);
      link.click();

      // Cleanup
      document.body.removeChild(link);
      window.URL.revokeObjectURL(url);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setDownloading(false);
    }
  };

  useEffect(() => {
    loadDirectoryTree();
  }, [sandboxId]);

  const toggleFolder = (path: string) => {
    const newExpanded = new Set(expandedFolders);
    if (newExpanded.has(path)) {
      newExpanded.delete(path);
    } else {
      newExpanded.add(path);
    }
    setExpandedFolders(newExpanded);
  };

  const getFileIcon = (node: FileNode) => {
    const extension = node.name.split('.').pop()?.toLowerCase();
    const iconSize = 16;

    if (node.is_dir) {
      return expandedFolders.has(node.path) ? (
        <FolderOpen size={iconSize} className="text-yellow-500" />
      ) : (
        <Folder size={iconSize} className="text-yellow-500" />
      );
    }

    // File icons based on extension
    switch (extension) {
      case 'js':
      case 'jsx':
      case 'ts':
      case 'tsx':
      case 'py':
      case 'rb':
      case 'go':
      case 'java':
      case 'c':
      case 'cpp':
      case 'h':
      case 'cs':
      case 'php':
      case 'swift':
      case 'kt':
      case 'rs':
        return <FileCode size={iconSize} className="text-blue-500" />;
      case 'json':
      case 'xml':
      case 'yaml':
      case 'yml':
      case 'toml':
        return <FileJson size={iconSize} className="text-green-500" />;
      case 'txt':
      case 'md':
      case 'rst':
      case 'log':
        return <FileText size={iconSize} className="text-gray-500" />;
      case 'png':
      case 'jpg':
      case 'jpeg':
      case 'gif':
      case 'svg':
      case 'ico':
      case 'webp':
        return <ImageIcon size={iconSize} className="text-purple-500" />;
      case 'mp4':
      case 'avi':
      case 'mov':
      case 'wmv':
      case 'flv':
      case 'webm':
        return <Film size={iconSize} className="text-pink-500" />;
      case 'mp3':
      case 'wav':
      case 'ogg':
      case 'flac':
        return <Music size={iconSize} className="text-orange-500" />;
      case 'zip':
      case 'tar':
      case 'gz':
      case 'rar':
      case '7z':
        return <Archive size={iconSize} className="text-red-500" />;
      default:
        return <File size={iconSize} className="text-gray-400" />;
    }
  };

  const getLanguage = (fileName: string): string => {
    const extension = fileName.split('.').pop()?.toLowerCase();
    switch (extension) {
      case 'js':
      case 'jsx':
        return 'javascript';
      case 'ts':
      case 'tsx':
        return 'typescript';
      case 'py':
        return 'python';
      case 'rb':
        return 'ruby';
      case 'go':
        return 'go';
      case 'java':
        return 'java';
      case 'c':
        return 'c';
      case 'cpp':
      case 'cc':
      case 'cxx':
        return 'cpp';
      case 'h':
      case 'hpp':
        return 'cpp';
      case 'cs':
        return 'csharp';
      case 'php':
        return 'php';
      case 'swift':
        return 'swift';
      case 'kt':
        return 'kotlin';
      case 'rs':
        return 'rust';
      case 'json':
        return 'json';
      case 'xml':
        return 'xml';
      case 'yaml':
      case 'yml':
        return 'yaml';
      case 'toml':
        return 'toml';
      case 'html':
      case 'htm':
        return 'html';
      case 'css':
        return 'css';
      case 'scss':
      case 'sass':
        return 'scss';
      case 'sql':
        return 'sql';
      case 'md':
        return 'markdown';
      case 'sh':
      case 'bash':
        return 'bash';
      case 'tsv':
      case 'csv':
        return 'csv';
      default:
        return 'text';
    }
  };

  const renderTreeNode = (node: FileNode, level: number = 0): React.ReactNode => {
    const isExpanded = expandedFolders.has(node.path);
    const isSelected = selectedFile?.path === node.path;

    return (
      <Box key={node.path}>
        <HStack
          spacing={2}
          pl={`${level * 16 + 8}px`}
          pr={4}
          py={1}
          cursor="pointer"
          _hover={{ bg: hoverBg }}
          bg={isSelected ? hoverBg : 'transparent'}
          onClick={() => {
            if (node.is_dir) {
              toggleFolder(node.path);
            } else {
              loadFileContent(node);
            }
          }}
          transition="background 0.15s"
        >
          {node.is_dir && (
            <Box display="flex" alignItems="center">
              {isExpanded ? (
                <ChevronDown size={14} />
              ) : (
                <ChevronRight size={14} />
              )}
            </Box>
          )}
          <Box display="flex" alignItems="center">
            {getFileIcon(node)}
          </Box>
          <Text fontSize="sm" flex={1} fontFamily="monospace">
            {node.name}
          </Text>
          {!node.is_dir && node.size !== undefined && (
            <Text fontSize="xs" color="gray.500">
              {formatFileSize(node.size)}
            </Text>
          )}
        </HStack>

        {node.is_dir && isExpanded && node.children && (
          <Box>
            {node.children.map((child) => renderTreeNode(child, level + 1))}
          </Box>
        )}
      </Box>
    );
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const supportsPreview = (fileName: string): boolean => {
    const extension = fileName.split('.').pop()?.toLowerCase();
    const previewableExtensions = ['html', 'htm', 'md', 'markdown', 'json', 'jsonl'];
    return previewableExtensions.includes(extension || '');
  };

  const resetViewMode = (file: FileNode) => {
    setSelectedFile(file);
    setPreviewError(null);
    // Auto-switch to preview mode if file supports it
    if (supportsPreview(file.name)) {
      setViewMode('preview');
    } else {
      setViewMode('raw');
    }
  };

  // Helper function to convert relative URLs to absolute URLs for API access
  // This is needed because blob URLs have a unique origin and the <base> tag
  // won't work due to cross-origin security policies
  const rewriteRelativeUrls = (html: string, sandboxId: string, filePath: string): string => {
    // Handle root-level files
    const dirPath = filePath.includes('/')
      ? filePath.substring(0, filePath.lastIndexOf('/'))
      : '';

    const baseURL = `${API_BASE_PATH}/static/${sandboxId}/${dirPath}`;
    // Ensure baseURL doesn't have trailing slash for root-level files
    const normalizedBaseURL = baseURL.endsWith('/') ? baseURL.slice(0, -1) : baseURL;

    // Rewrite href/src attributes in various HTML tags
    // Match relative URLs (those that don't start with http://, https://, /, data:, blob:, about:, etc.)
    const processedHtml = html
      // Rewrite src attributes in img, script, source, iframe, video, audio
      // Handles: <img src="...">, <script src="...">, etc.
      .replace(/(<(?:img|script|source|iframe|video|audio|embed)[^>]*)\s+src=(["'])(?!data:|blob:|about:|#|\/|https?:\/\/)([^"']+)\2/gi,
        `$1 src="${normalizedBaseURL}/$3"`)
      // Rewrite href attributes in link tags (css, favicon, etc.)
      // Handles: <link href="...">, <link rel="..." href="...">
      // Note: \s+ requires at least one whitespace before href
      .replace(/(<link[^>]*)\s+href=(["'])(?!data:|blob:|about:|#|\/|https?:\/\/)([^"']+)\2/gi,
        `$1 href="${normalizedBaseURL}/$3"`)
      // Rewrite href attributes in a and area tags
      .replace(/(<(?:a|area)[^>]*)\s+href=(["'])(?!data:|blob:|about:|#|\/|https?:\/\/)([^"']+)\2/gi,
        `$1 href="${normalizedBaseURL}/$3"`);

    return processedHtml;
  };

  const renderPreview = () => {
    if (!selectedFile) {
      return (
        <Box
          display="flex"
          alignItems="center"
          justifyContent="center"
          h="100%"
          bg={treeBg}
        >
          <VStack spacing={4}>
            <File size={48} className="text-gray-400" />
            <Text color="gray.500">Select a file to preview</Text>
          </VStack>
        </Box>
      );
    }

    if (loadingContent) {
      return (
        <Box
          display="flex"
          alignItems="center"
          justifyContent="center"
          h="100%"
          bg={treeBg}
        >
          <VStack spacing={4}>
            <Spinner size="xl" />
            <Text>Loading file...</Text>
          </VStack>
        </Box>
      );
    }

    const extension = selectedFile.name.split('.').pop()?.toLowerCase();
    const imageExtensions = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico', 'webp'];
    const isImage = imageExtensions.includes(extension || '');
    const canPreview = supportsPreview(selectedFile.name);

    // Handle image files (always show in raw mode)
    if (isImage) {
      return (
        <Box
          p={4}
          bg={treeBg}
          h="100%"
          overflow="auto"
        >
          <VStack spacing={4}>
            <Text fontWeight="bold" fontSize="lg">
              {selectedFile.name}
            </Text>
            <Box
              bg="white"
              p={2}
              borderRadius="md"
              boxShadow="sm"
            >
              <img
                src={`${API_BASE_PATH}/static/${sandboxId}/${selectedFile.path}`}
                alt={selectedFile.name}
                style={{ maxWidth: '100%', maxHeight: '600px' }}
              />
            </Box>
          </VStack>
        </Box>
      );
    }

    const language = getLanguage(selectedFile.name);
    const isCodeFile = language !== 'text';

    // Render header with toggle buttons
    const renderHeader = () => (
      <Box p={4} borderBottom="1px" borderColor={borderColor}>
        <VStack spacing={3} align="stretch">
          <HStack justify="space-between">
            <Text fontWeight="bold" fontSize="lg" fontFamily="monospace">
              {selectedFile.name}
            </Text>
            <Text fontSize="sm" color="gray.500">
              {selectedFile.size !== undefined && formatFileSize(selectedFile.size)}
            </Text>
          </HStack>
          <HStack justify="center">
            <ButtonGroup size="sm" isAttached variant="outline">
              <Button
                leftIcon={<Code size={14} />}
                colorScheme={viewMode === 'raw' ? 'blue' : 'gray'}
                onClick={() => {
                  setViewMode('raw');
                  setPreviewError(null);
                }}
              >
                Raw
              </Button>
              {canPreview && (
                <Button
                  leftIcon={<Eye size={14} />}
                  colorScheme={viewMode === 'preview' ? 'blue' : 'gray'}
                  onClick={() => {
                    setViewMode('preview');
                    setPreviewError(null);
                  }}
                >
                  Preview
                </Button>
              )}
              <Button
                leftIcon={<ExternalLink size={14} />}
                onClick={() => {
                  const fileUrl = `${API_BASE_PATH}/static/${sandboxId}/${selectedFile.path}`;
                  window.open(fileUrl, '_blank', 'noopener,noreferrer');
                }}
              >
                View
              </Button>
            </ButtonGroup>
          </HStack>
        </VStack>
      </Box>
    );

    // Render preview content based on file type
    const renderPreviewContent = () => {
      if (previewError) {
        return (
          <Box p={4}>
            <Text color="red.500">Preview failed: {previewError}</Text>
            <Text fontSize="sm" color="gray.500" mt={2}>
              Switch to Raw mode to see the content
            </Text>
          </Box>
        );
      }

      try {
        // HTML files - render in sandboxed iframe with CSS/JS support
        if (extension === 'html' || extension === 'htm') {
          // Rewrite relative URLs to absolute API URLs
          // This is necessary because blob URLs have a unique origin and the <base> tag
          // won't work due to cross-origin security policies
          const htmlWithRewrittenUrls = rewriteRelativeUrls(fileContent, sandboxId, selectedFile.path);

          // Configure DOMPurify to allow style, script, and link tags
          const sanitizedHTML = DOMPurify.sanitize(htmlWithRewrittenUrls, {
            USE_PROFILES: { html: true },
            ADD_TAGS: ['style', 'script', 'link'],
            ADD_ATTR: ['src', 'href', 'type', 'rel', 'media', 'defer', 'async'],
            FORBID_TAGS: ['iframe', 'object', 'embed', 'form'],
            FORBID_ATTR: ['onerror', 'onload', 'onclick', 'onmouseover'],
          });

          return (
            <Box h="calc(100% - 120px)">
              <iframe
                key={selectedFile.path}
                srcdoc={sanitizedHTML}
                style={{
                  width: '100%',
                  height: '100%',
                  border: 'none',
                  borderRadius: '4px',
                }}
                sandbox="allow-scripts allow-same-origin"
                title="HTML Preview"
              />
            </Box>
          );
        }

        // Markdown files - render with react-markdown
        if (extension === 'md' || extension === 'markdown') {
          return (
            <Box
              p={6}
              overflow="auto"
              h="calc(100% - 120px)"
              css={{
                '& h1': { fontSize: '2xl', fontWeight: 'bold', mt: 4, mb: 2 },
                '& h2': { fontSize: 'xl', fontWeight: 'bold', mt: 3, mb: 2 },
                '& h3': { fontSize: 'lg', fontWeight: 'bold', mt: 2, mb: 1 },
                '& p': { mb: 2 },
                '& code': {
                  bg: 'gray.100',
                  px: 1,
                  rounded: 'md',
                  fontFamily: 'monospace',
                  fontSize: 'sm',
                },
                '& pre': {
                  bg: 'gray.900',
                  color: 'gray.100',
                  p: 3,
                  rounded: 'md',
                  overflow: 'auto',
                  mb: 2,
                },
                '& pre code': {
                  bg: 'transparent',
                  color: 'inherit',
                  p: 0,
                },
                '& ul, & ol': { ml: 6, mb: 2 },
                '& li': { mb: 1 },
                '& a': { color: 'blue.500' },
                '& blockquote': {
                  borderLeft: '4px solid',
                  borderColor: 'gray.300',
                  pl: 4,
                  fontStyle: 'italic',
                  mb: 2,
                },
                '& table': {
                  borderCollapse: 'collapse',
                  width: '100%',
                  mb: 2,
                },
                '& th, & td': {
                  border: '1px solid',
                  borderColor: 'gray.300',
                  padding: '8px',
                  textAlign: 'left',
                },
                '& th': {
                  bg: 'gray.100',
                  fontWeight: 'bold',
                },
              }}
            >
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {fileContent}
              </ReactMarkdown>
            </Box>
          );
        }

        // JSON files - pretty print
        if (extension === 'json') {
          try {
            const parsed = JSON.parse(fileContent);
            const prettyJSON = JSON.stringify(parsed, null, 2);
            return (
              <Box p={4} overflow="auto" h="calc(100% - 120px)">
                <SyntaxHighlighter
                  language="json"
                  style={vscDarkPlus}
                  customStyle={{
                    margin: 0,
                    borderRadius: 'md',
                  }}
                  showLineNumbers
                  lineNumberStyle={{ fontSize: '12px' }}
                >
                  {prettyJSON}
                </SyntaxHighlighter>
              </Box>
            );
          } catch (err) {
            throw new Error('Invalid JSON format');
          }
        }

        // JSONL files - line by line JSON
        if (extension === 'jsonl') {
          try {
            const lines = fileContent.trim().split('\n');
            const parsedLines = lines.map((line, index) => {
              try {
                const parsed = JSON.parse(line);
                return `${index + 1}: ${JSON.stringify(parsed, null, 2)}`;
              } catch {
                return `${index + 1}: ${line}`;
              }
            });

            return (
              <Box p={4} overflow="auto" h="calc(100% - 120px)">
                <SyntaxHighlighter
                  language="json"
                  style={vscDarkPlus}
                  customStyle={{
                    margin: 0,
                    borderRadius: 'md',
                  }}
                  showLineNumbers
                  lineNumberStyle={{ fontSize: '12px' }}
                >
                  {parsedLines.join('\n')}
                </SyntaxHighlighter>
              </Box>
            );
          } catch (err) {
            throw new Error('Invalid JSONL format');
          }
        }

        return (
          <Box p={4}>
            <Text color="gray.500">Preview not available for this file type</Text>
          </Box>
        );
      } catch (err: any) {
        setPreviewError(err.message || 'Preview failed');
        return (
          <Box p={4}>
            <Text color="red.500">Preview failed: {err.message}</Text>
            <Text fontSize="sm" color="gray.500" mt={2}>
              Switch to Raw mode to see the content
            </Text>
          </Box>
        );
      }
    };

    // Render raw content
    const renderRawContent = () => {
      return (
        <Box overflow="auto" style={{ height: 'calc(100% - 120px)' }}>
          {isCodeFile ? (
            <SyntaxHighlighter
              language={language}
              style={vscDarkPlus}
              customStyle={{
                margin: 0,
                borderRadius: 0,
              }}
              showLineNumbers
              lineNumberStyle={{ fontSize: '12px' }}
            >
              {fileContent}
            </SyntaxHighlighter>
          ) : (
            <Box p={4}>
              <Text
                fontFamily="monospace"
                fontSize="sm"
                whiteSpace="pre-wrap"
                wordBreak="break-word"
              >
                {fileContent}
              </Text>
            </Box>
          )}
        </Box>
      );
    };

    return (
      <Box
        h="100%"
        bg={treeBg}
        overflow="hidden"
      >
        {renderHeader()}
        {canPreview && viewMode === 'preview' ? renderPreviewContent() : renderRawContent()}
      </Box>
    );
  };

  return (
    <Box
      bg={bgColor}
      borderRadius="lg"
      borderWidth="1px"
      borderColor={borderColor}
      h="600px"
      overflow="hidden"
    >
      {isPanelSizeLoaded && (
        <PanelGroup
          direction="horizontal"
          autoSaveId="dsb-minifs-layout"
          onLayout={(sizes: number[]) => {
            const leftSize = sizes[0];
            if (leftSize >= PANEL_SIZE_CONSTRAINTS.MIN_SIZE && leftSize <= PANEL_SIZE_CONSTRAINTS.MAX_SIZE) {
              setLeftPanelSize(leftSize);
              setIsLeftPanelCollapsed(leftSize <= PANEL_SIZE_CONSTRAINTS.COLLAPSED_THRESHOLD);
            }
          }}
        >
          {/* Left Panel - File Explorer */}
          <Panel
            ref={leftPanelRef}
            defaultSize={leftPanelSize}
            minSize={20}
            maxSize={80}
            collapsible={true}
          >
            <Box
              minW="300px"
              borderRight="1px"
              borderColor={borderColor}
              display="flex"
              flexDirection="column"
              h="full"
              position="relative"
            >
              {/* Header */}
              <Box
                p={4}
                borderBottom="1px"
                borderColor={borderColor}
                bg={treeBg}
              >
                <HStack justify="space-between">
                  <Text fontWeight="bold" fontSize="md">
                    File Explorer
                  </Text>
                  <HStack>
                    {/* Collapse button - only show when panel is expanded */}
                    {!isLeftPanelCollapsed && (
                      <Tooltip label="Collapse panel">
                        <IconButton
                          aria-label="Collapse panel"
                          size="sm"
                          variant="ghost"
                          icon={<ChevronsLeft size={16} />}
                          onClick={handleCollapse}
                        />
                      </Tooltip>
                    )}
                    <Tooltip label="Download all files as ZIP">
                      <IconButton
                        aria-label="Download all files"
                        size="sm"
                        variant="ghost"
                        icon={<Download size={16} />}
                        onClick={downloadAllFiles}
                        isLoading={downloading}
                      />
                    </Tooltip>
                    <Tooltip label="Refresh">
                      <IconButton
                        aria-label="Refresh"
                        size="sm"
                        variant="ghost"
                        icon={<RefreshCw size={16} />}
                        onClick={loadDirectoryTree}
                        isLoading={loading}
                      />
                    </Tooltip>
                  </HStack>
                </HStack>
              </Box>

              {/* Tree */}
              <Box flex={1} overflow="auto">
                {loading ? (
                  <Box
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    h="200px"
                  >
                    <VStack spacing={4}>
                      <Spinner size="xl" />
                      <Text>Loading files...</Text>
                    </VStack>
                  </Box>
                ) : error ? (
                  <Box p={4}>
                    <Text color="red.500">{error}</Text>
                  </Box>
                ) : tree.length === 0 ? (
                  <Box
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    h="200px"
                  >
                    <Text color="gray.500">No files found</Text>
                  </Box>
                ) : (
                  <VStack spacing={0} align="stretch">
                    {tree.map((node) => renderTreeNode(node))}
                  </VStack>
                )}
              </Box>
            </Box>
          </Panel>

          {/* Resize Handle */}
          <PanelResizeHandle>
            <MiniFsResizeHandle />
          </PanelResizeHandle>

          {/* Right Panel - File Preview */}
          <Panel defaultSize={100 - leftPanelSize} minSize={20}>
            <Box display="flex" flexDirection="column" overflow="hidden" h="full" position="relative">
              {/* Expand button - only show when left panel is collapsed */}
              {isLeftPanelCollapsed && (
                <Box
                  position="absolute"
                  left={4}
                  top={4}
                  zIndex={5}
                >
                  <Tooltip label="Expand panel">
                    <IconButton
                      aria-label="Expand panel"
                      size="sm"
                      variant="ghost"
                      icon={<ChevronsRight size={16} />}
                      onClick={handleExpand}
                    />
                  </Tooltip>
                </Box>
              )}
              {renderPreview()}
            </Box>
          </Panel>
        </PanelGroup>
      )}
    </Box>
  );
}
