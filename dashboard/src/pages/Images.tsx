// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useState } from 'react';
import {
  Heading,
  SimpleGrid,
  Box,
  Text,
  HStack,
  Button,
  VStack,
  useColorModeValue,
  Spinner,
  Badge,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalCloseButton,
  Input,
  FormControl,
  FormLabel,
  useDisclosure,
} from '@chakra-ui/react';
import { Plus, Trash2, Download } from 'lucide-react';
import { apiClient } from '../api/client';
import { formatBytes, formatRelativeTime } from '../utils/formatters';
import { ImagePullProgress } from '../components/images/ImagePullProgress';

export default function Images() {
  const [images, setImages] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const { isOpen: isPullModalOpen, onOpen: onPullModalOpen, onClose: onPullModalClose } = useDisclosure();
  const { isOpen: isProgressOpen, onOpen: onProgressOpen, onClose: onProgressClose } = useDisclosure();
  const [pullImageName, setPullImageName] = useState('');
  const [pullTag, setPullTag] = useState('latest');

  const bgColor = useColorModeValue('white', 'gray.800');

  const loadImages = async () => {
    try {
      setLoading(true);
      const data = await apiClient.listImages();
      setImages(data);
      setError(null);
    } catch (err: any) {
      setError(err.message || 'Failed to load images');
    } finally {
      setLoading(false);
    }
  };

  React.useEffect(() => {
    loadImages();
  }, []);

  const handleDelete = async (imageId: string) => {
    if (!confirm('Are you sure you want to delete this image?')) return;

    try {
      await apiClient.deleteImage(imageId);
      loadImages();
    } catch (err: any) {
      alert(`Failed to delete image: ${err.message}`);
    }
  };

  const handlePull = async () => {
    if (!pullImageName.trim()) return;

    // Close the pull modal
    onPullModalClose();

    // Open the progress modal
    onProgressOpen();
  };

  if (loading) {
    return (
      <Box p={8} textAlign="center">
        <Spinner size="xl" />
        <Text mt={4}>Loading images...</Text>
      </Box>
    );
  }

  if (error) {
    return (
      <Box p={4} bg="red.50" borderRadius="md">
        <Text color="red.500">{error}</Text>
      </Box>
    );
  }

  return (
    <VStack align="stretch" spacing={6}>
      {/* Header */}
      <HStack justify="space-between">
        <Heading size="lg">Docker Images</Heading>
        <Button colorScheme="blue" leftIcon={<Download size={18} />} onClick={onPullModalOpen}>
          Pull Image
        </Button>
      </HStack>

      {/* Image Grid */}
      {images.length === 0 ? (
        <Box p={8} textAlign="center" bg={bgColor} borderRadius="lg">
          <Text color="gray.500">No images found</Text>
          <Button mt={4} colorScheme="blue" size="sm" onClick={onPullModalOpen}>
            Pull your first image
          </Button>
        </Box>
      ) : (
        <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={4}>
          {images.map((image) => (
            <ImageCard
              key={image.id}
              image={image}
              onDelete={() => handleDelete(image.id)}
            />
          ))}
        </SimpleGrid>
      )}

      {/* Pull Image Modal */}
      <Modal isOpen={isPullModalOpen} onClose={onPullModalClose}>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Pull Docker Image</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <VStack spacing={4}>
              <FormControl>
                <FormLabel>Image Name</FormLabel>
                <Input
                  placeholder="e.g., alpine, nginx, ubuntu"
                  value={pullImageName}
                  onChange={(e) => setPullImageName(e.target.value)}
                />
              </FormControl>
              <FormControl>
                <FormLabel>Tag (optional)</FormLabel>
                <Input
                  placeholder="e.g., latest, alpine, 3.18"
                  value={pullTag}
                  onChange={(e) => setPullTag(e.target.value)}
                />
              </FormControl>
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={onPullModalClose}>
              Cancel
            </Button>
            <Button
              colorScheme="blue"
              onClick={handlePull}
              isDisabled={!pullImageName.trim()}
            >
              Pull Image
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* Pull Progress Modal */}
      <ImagePullProgress
        isOpen={isProgressOpen}
        onClose={onProgressClose}
        imageName={pullImageName}
        tag={pullTag || 'latest'}
        onComplete={loadImages}
      />
    </VStack>
  );
}

interface ImageCardProps {
  image: any;
  onDelete: () => void;
}

function ImageCard({ image, onDelete }: ImageCardProps) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  return (
    <Box bg={bg} borderWidth="1px" borderColor={borderColor} borderRadius="lg" p={4}>
      <VStack align="stretch" spacing={3}>
        {/* Image Name */}
        <Text fontWeight="bold" noOfLines={1}>
          {image.repo_tags?.[0] || image.id.slice(0, 12)}
        </Text>

        {/* Size & Age */}
        <HStack fontSize="sm" color="gray.500">
          <Text>{formatBytes(image.size)}</Text>
          <Text>•</Text>
          <Text>{formatRelativeTime(new Date(image.created * 1000))}</Text>
        </HStack>

        {/* Actions */}
        <HStack justify="flex-end">
          <Button
            size="sm"
            variant="ghost"
            colorScheme="red"
            leftIcon={<Trash2 size={14} />}
            onClick={onDelete}
          >
            Delete
          </Button>
        </HStack>
      </VStack>
    </Box>
  );
}
