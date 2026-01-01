import numpy as np

DATA_PATH = '/Users/vitalyvorobyev/vision/data/stereo'

def main():
    """ Run me """
    data = np.load(f'{DATA_PATH}/out/parameters.npz')
    print(list(data.keys()))

    print(data['BoardSize'], data['SquareSize'])
    print(data['Objpoints'])
    print(data['Transformation'])

if __name__ == '__main__':
    main()
